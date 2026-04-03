use anyhow::{anyhow, bail, Context, Result};
use std::{fs, path::Path};

use agent_core::{AttachmentKind, InputAttachment};

pub(super) struct LoadedImageAttachment {
    pub(super) mime_type: &'static str,
    pub(super) data_base64: String,
}

pub(super) fn load_image_attachment(attachment: &InputAttachment) -> Result<LoadedImageAttachment> {
    match attachment.kind {
        AttachmentKind::Image => load_image_attachment_from_path(&attachment.path),
        AttachmentKind::File => bail!(
            "provider attachment '{}' is a generic file; file attachments are only supported through hosted tool flows",
            attachment.path.display()
        ),
    }
}

pub(super) fn load_image_attachment_from_path(path: &Path) -> Result<LoadedImageAttachment> {
    let mime_type = infer_image_mime_type(path)?;
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read image attachment from {}", path.display()))?;
    Ok(LoadedImageAttachment {
        mime_type,
        data_base64: encode_base64(&bytes),
    })
}

fn infer_image_mime_type(path: &Path) -> Result<&'static str> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or_else(|| {
            anyhow!(
                "image attachment '{}' is missing a file extension",
                path.display()
            )
        })?;

    match extension.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "gif" => Ok("image/gif"),
        "webp" => Ok("image/webp"),
        _ => bail!(
            "image attachment '{}' uses unsupported extension '.{}'",
            path.display(),
            extension
        ),
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | (bytes[index + 2] as u32);
        encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
        encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        encoded.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
        encoded.push(TABLE[(block & 0x3f) as usize] as char);
        index += 3;
    }

    match bytes.len() - index {
        1 => {
            let block = (bytes[index] as u32) << 16;
            encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
            encoded.push('=');
            encoded.push('=');
        }
        2 => {
            let block = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
            encoded.push('=');
        }
        _ => {}
    }

    encoded
}
