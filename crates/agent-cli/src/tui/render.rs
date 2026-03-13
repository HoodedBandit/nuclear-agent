use chrono::Local;
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use agent_core::{PermissionPreset, SessionMessage};

use crate::{format_session_message_for_display, permission_summary, thinking_level_label};

use super::app::{OverlayState, PickerMode, TuiApp};

const MIN_TRANSCRIPT_WINDOW_VISUAL_LINES: usize = 400;
const TRANSCRIPT_WINDOW_BUFFER_VIEWPORTS: usize = 4;

struct TranscriptViewport {
    lines: Vec<Line<'static>>,
    scroll_top: usize,
    total_visual_lines: usize,
}

pub(super) fn draw_app(frame: &mut Frame<'_>, app: &TuiApp<'_>) {
    let show_header = app.transcript.is_empty();
    let composer_lines = render_composer_lines(app);
    let composer_content_height =
        u16::try_from(wrapped_line_count(&composer_lines, frame.area().width).max(1)).unwrap_or(1);
    let composer_height = composer_content_height.saturating_add(2);

    let mut constraints = Vec::new();
    if show_header {
        constraints.push(Constraint::Length(5));
    }
    constraints.push(Constraint::Min(1));
    constraints.push(Constraint::Length(composer_height));
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Length(1));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    let mut section_index = 0usize;
    if show_header {
        let header = Paragraph::new(render_empty_header(app))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(header, layout[section_index]);
        section_index += 1;
    }

    let transcript_area = layout[section_index];
    let transcript_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(
            " Conversation {} ",
            app.session_id
                .as_deref()
                .map(|id| {
                    if id.len() > 8 {
                        &id[..8]
                    } else {
                        id
                    }
                })
                .unwrap_or("new")
        ));
    let transcript_inner = transcript_block.inner(transcript_area);
    frame.render_widget(transcript_block, transcript_area);
    let transcript_viewport = transcript_viewport(
        render_transcript_lines(&app.transcript),
        transcript_inner.width,
        transcript_inner.height,
        app.transcript_scroll_back,
    );
    let top_padding = bottom_padding_line_count(
        &transcript_viewport.lines,
        transcript_inner.width,
        transcript_inner.height,
    );
    let mut transcript_display_lines =
        Vec::with_capacity(top_padding + transcript_viewport.lines.len());
    transcript_display_lines.extend((0..top_padding).map(|_| Line::from(String::new())));
    transcript_display_lines.extend(transcript_viewport.lines);
    let transcript_scroll = clamp_scroll_top(
        &transcript_display_lines,
        transcript_inner.width,
        transcript_inner.height,
        top_padding.saturating_add(transcript_viewport.scroll_top),
    );
    let transcript = Paragraph::new(transcript_display_lines)
        .wrap(Wrap { trim: false })
        .scroll((u16::try_from(transcript_scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(transcript, transcript_inner);
    section_index += 1;

    let composer_area = layout[section_index];
    let composer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Prompt ");
    let composer_inner = composer_block.inner(composer_area);
    frame.render_widget(composer_block, composer_area);
    frame.render_widget(
        Paragraph::new(composer_lines).wrap(Wrap { trim: false }),
        composer_inner,
    );
    section_index += 1;

    let footer_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(64)])
        .split(layout[section_index]);
    frame.render_widget(Paragraph::new(render_footer_left(app)), footer_layout[0]);
    frame.render_widget(
        Paragraph::new(render_footer_right(app)).alignment(Alignment::Right),
        footer_layout[1],
    );
    section_index += 1;

    let status_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(56)])
        .split(layout[section_index]);
    frame.render_widget(Paragraph::new(render_status_left(app)), status_layout[0]);
    frame.render_widget(
        Paragraph::new(render_status_right(app)).alignment(Alignment::Right),
        status_layout[1],
    );

    if app.overlay.is_none() && app.picker.is_none() {
        if let Some(position) = composer_cursor_position(app, composer_inner) {
            frame.set_cursor_position(position);
        }
    }

    if let Some(overlay) = &app.overlay {
        render_overlay(frame, app, overlay);
        if let Some(position) = overlay_cursor_position(frame.area(), overlay) {
            frame.set_cursor_position(position);
        }
    }

    if let Some(picker) = &app.picker {
        let area = centered_rect(90, 70, frame.area());
        frame.render_widget(Clear, area);
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(1),
            ])
            .split(area);
        let query = Paragraph::new(picker.query.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Search "),
        );
        frame.render_widget(query, sections[0]);
        let (items, total) = match picker.mode {
            PickerMode::Resume | PickerMode::Fork => {
                let sessions = picker.filtered_sessions();
                let (start, end) =
                    picker_visible_range(sessions.len(), picker.selected, sections[1].height);
                let items = sessions[start..end]
                    .iter()
                    .enumerate()
                    .map(|(index, session)| {
                        let actual_index = start + index;
                        let title = session.title.as_deref().unwrap_or("(untitled)");
                        let cwd = session
                            .cwd
                            .as_deref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let marker = if actual_index == picker.selected {
                            ">"
                        } else {
                            " "
                        };
                        ListItem::new(format!(
                            "{} {:<20} {:<12} {:<28} {}",
                            marker, title, session.alias, cwd, session.updated_at
                        ))
                    })
                    .collect::<Vec<_>>();
                (items, sessions.len())
            }
            PickerMode::Model => {
                let models = picker.filtered_models();
                let (start, end) =
                    picker_visible_range(models.len(), picker.selected, sections[1].height);
                let items = models[start..end]
                    .iter()
                    .enumerate()
                    .map(|(index, model)| {
                        let actual_index = start + index;
                        let marker = if actual_index == picker.selected {
                            ">"
                        } else {
                            " "
                        };
                        let current_marker = if app.active_model.as_deref()
                            == Some(model.display_name.as_str())
                            || app.requested_model.as_deref() == Some(model.id.as_str())
                        {
                            " *"
                        } else {
                            ""
                        };
                        let context_suffix =
                            match (model.context_window, model.effective_context_window_percent) {
                                (Some(window), Some(percent)) => {
                                    format!(
                                        " | ctx {} @ {}%",
                                        format_tokens_compact(window),
                                        percent
                                    )
                                }
                                (Some(window), None) => {
                                    format!(" | ctx {}", format_tokens_compact(window))
                                }
                                _ => String::new(),
                            };
                        let description = model
                            .description
                            .as_deref()
                            .map(|text| format!(" | {text}"))
                            .unwrap_or_default();
                        ListItem::new(format!(
                            "{marker} {} ({}){current_marker}{context_suffix}{description}",
                            model.display_name, model.id
                        ))
                    })
                    .collect::<Vec<_>>();
                (items, models.len())
            }
            PickerMode::Alias
            | PickerMode::Thinking
            | PickerMode::Permissions
            | PickerMode::Config
            | PickerMode::Delegation
            | PickerMode::Autonomy
            | PickerMode::Provider
            | PickerMode::ProviderAction
            | PickerMode::Webhook
            | PickerMode::WebhookAction
            | PickerMode::Inbox
            | PickerMode::InboxAction
            | PickerMode::Telegram
            | PickerMode::TelegramAction
            | PickerMode::Discord
            | PickerMode::DiscordAction
            | PickerMode::Slack
            | PickerMode::SlackAction
            | PickerMode::Signal
            | PickerMode::SignalAction
            | PickerMode::HomeAssistant
            | PickerMode::HomeAssistantAction
            | PickerMode::Persistence
            | PickerMode::SkillDraft
            | PickerMode::SkillDraftAction => {
                let entries = picker.filtered_items();
                let (start, end) =
                    picker_visible_range(entries.len(), picker.selected, sections[1].height);
                let items = entries[start..end]
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let actual_index = start + index;
                        let marker = if actual_index == picker.selected {
                            ">"
                        } else {
                            " "
                        };
                        let current_marker = if item.current { " *" } else { "" };
                        let detail = item
                            .detail
                            .as_deref()
                            .map(|text| format!(" | {text}"))
                            .unwrap_or_default();
                        ListItem::new(format!("{marker} {}{current_marker}{detail}", item.label))
                    })
                    .collect::<Vec<_>>();
                (items, entries.len())
            }
        };
        let selected_display = if total == 0 {
            0
        } else {
            picker.selected.saturating_add(1).min(total)
        };
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " {} ({}/{}) ",
                    picker.title, selected_display, total
                )),
        );
        frame.render_widget(list, sections[1]);
        frame.render_widget(
            Paragraph::new(picker.hint.as_str()).alignment(Alignment::Center),
            sections[2],
        );
    }
}

fn render_transcript_lines(messages: &[SessionMessage]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for message in messages {
        let rendered = format_session_message_for_display(message);
        let timestamp = message
            .created_at
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string();
        match message.role {
            agent_core::MessageRole::User => {
                if !lines.is_empty() {
                    lines.push(Line::from(String::new()));
                }
                lines.push(Line::from(format!("[you] {timestamp}")));
                for line in rendered.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
                for attachment in &message.attachments {
                    lines.push(Line::from(format!(
                        "  [image] {}",
                        attachment.path.display()
                    )));
                }
            }
            agent_core::MessageRole::Assistant => {
                if !lines.is_empty() {
                    lines.push(Line::from(String::new()));
                }
                let model = message.model.as_deref().unwrap_or("assistant");
                lines.push(Line::from(format!("[assistant: {model}] {timestamp}")));
                for line in rendered.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
            }
            agent_core::MessageRole::Tool => {
                if !lines.is_empty() {
                    lines.push(Line::from(String::new()));
                }
                let tool_name = message.tool_name.as_deref().unwrap_or("tool");
                lines.push(Line::from(format!("[tool: {tool_name}] {timestamp}")));
                for line in rendered.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
            }
            agent_core::MessageRole::System => {
                if !lines.is_empty() {
                    lines.push(Line::from(String::new()));
                }
                lines.push(Line::from(format!("[system] {timestamp}")));
                for line in rendered.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
            }
        }
    }
    lines
}

fn render_overlay(frame: &mut Frame<'_>, app: &TuiApp<'_>, overlay: &OverlayState) {
    let area = match overlay {
        OverlayState::Input { .. } => centered_rect(72, 34, frame.area()),
        _ => centered_rect(92, 86, frame.area()),
    };
    frame.render_widget(Clear, area);
    let title = match overlay {
        OverlayState::Transcript { .. } => " Transcript ".to_string(),
        OverlayState::Static { title, .. } => format!(" {title} "),
        OverlayState::Input { title, .. } => format!(" {title} "),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    match overlay {
        OverlayState::Transcript { scroll_back } => {
            let transcript_viewport = transcript_viewport(
                render_transcript_lines(&app.transcript),
                sections[0].width,
                sections[0].height,
                *scroll_back,
            );
            let percent = scroll_percent_for_total(
                transcript_viewport.total_visual_lines,
                usize::from(sections[0].height.max(1)),
                transcript_viewport.scroll_top,
            );
            frame.render_widget(
                Paragraph::new(transcript_viewport.lines)
                    .wrap(Wrap { trim: false })
                    .scroll((
                        u16::try_from(transcript_viewport.scroll_top).unwrap_or(u16::MAX),
                        0,
                    )),
                sections[0],
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "Esc close | Up/Down scroll | PageUp/PageDown jump | Home/End top/bottom | {}%",
                    percent
                ))
                .alignment(Alignment::Center),
                sections[1],
            );
        }
        OverlayState::Static { body, scroll, .. } => {
            let lines = if body.is_empty() {
                vec![Line::from("")]
            } else {
                body.lines()
                    .map(|line| Line::from(line.to_string()))
                    .collect::<Vec<_>>()
            };
            let clamped_scroll =
                clamp_scroll_top(&lines, sections[0].width, sections[0].height, *scroll);
            let percent = scroll_percent(
                &lines,
                sections[0].width,
                sections[0].height,
                clamped_scroll,
            );
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((u16::try_from(clamped_scroll).unwrap_or(u16::MAX), 0)),
                sections[0],
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "Esc close | Up/Down scroll | PageUp/PageDown jump | Home/End top/bottom | {}%",
                    percent
                ))
                .alignment(Alignment::Center),
                sections[1],
            );
        }
        OverlayState::Input {
            prompt,
            value,
            secret,
            ..
        } => {
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(inner);
            let displayed = if *secret {
                "*".repeat(value.chars().count())
            } else {
                value.clone()
            };
            frame.render_widget(
                Paragraph::new(prompt.as_str()).wrap(Wrap { trim: false }),
                sections[0],
            );
            frame.render_widget(
                Paragraph::new(displayed).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Value "),
                ),
                sections[1],
            );
            frame.render_widget(
                Paragraph::new("Enter save | Esc cancel | Ctrl+A/Ctrl+E move cursor")
                    .alignment(Alignment::Center),
                sections[2],
            );
        }
    }
}

fn overlay_cursor_position(area: Rect, overlay: &OverlayState) -> Option<Position> {
    let OverlayState::Input { value, cursor, .. } = overlay else {
        return None;
    };
    let area = centered_rect(72, 34, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let input_inner = input_block.inner(sections[1]);
    let visible_columns = usize::from(input_inner.width.saturating_sub(1));
    let cursor_chars = value[..(*cursor).min(value.len())].chars().count();
    let column = u16::try_from(cursor_chars.min(visible_columns)).unwrap_or(u16::MAX);
    Some(Position::new(
        input_inner.x.saturating_add(column),
        input_inner.y,
    ))
}

fn render_empty_header(app: &TuiApp<'_>) -> Vec<Line<'static>> {
    let model_label = app
        .active_model
        .clone()
        .or_else(|| app.requested_model.clone())
        .or_else(|| app.alias.clone())
        .unwrap_or_else(|| "main".to_string());
    vec![
        Line::from(format!(" >_ Autism CLI (v{})", env!("CARGO_PKG_VERSION"))),
        Line::from(String::new()),
        Line::from(format!(
            " model:     {} {}   /model to change",
            model_label,
            thinking_level_label(app.thinking_level)
        )),
        Line::from(format!(" directory: {}", app.cwd.display())),
    ]
}

fn render_composer_lines(app: &TuiApp<'_>) -> Vec<Line<'static>> {
    if app.input.is_empty() {
        return vec![Line::from("> Ask Autism to do anything")];
    }

    let mut lines = Vec::new();
    for (index, line) in app.input.lines().enumerate() {
        if index == 0 {
            lines.push(Line::from(format!("> {line}")));
        } else {
            lines.push(Line::from(format!("  {line}")));
        }
    }
    if app.input.ends_with('\n') {
        lines.push(Line::from("  "));
    }
    lines
}

fn render_footer_left(app: &TuiApp<'_>) -> String {
    if app.input.is_empty() {
        "  ? shortcuts | /config settings | /dashboard web ui | ctrl+t transcript | enter send"
            .to_string()
    } else {
        "  enter send | ctrl+j newline | /config settings | /dashboard web ui | ctrl+t transcript"
            .to_string()
    }
}

fn render_footer_right(app: &TuiApp<'_>) -> String {
    format!(
        "{} | {} att. | {}",
        permission_summary(app.permission_preset.unwrap_or(PermissionPreset::AutoEdit)),
        app.attachments.len(),
        app.alias.as_deref().unwrap_or("main"),
    )
}

fn render_status_left(app: &TuiApp<'_>) -> String {
    let session = app.session_id.as_deref().unwrap_or("(new)");
    let session_short = if session.len() > 12 {
        &session[..12]
    } else {
        session
    };
    let timestamp = Local::now().format("%H:%M").to_string();

    if app.busy {
        let elapsed = app
            .busy_since
            .map(|started| started.elapsed().as_secs())
            .unwrap_or(0);
        format!(
            "  working {} | session {} | {}",
            fmt_elapsed(elapsed),
            session_short,
            timestamp
        )
    } else {
        match app.latest_event_summary() {
            Some(event) => format!(
                "  ready | session {} | {} | {}",
                session_short, timestamp, event
            ),
            None => format!("  ready | session {} | {}", session_short, timestamp),
        }
    }
}

fn render_status_right(app: &TuiApp<'_>) -> String {
    let mut parts = Vec::new();
    parts.push(
        app.active_model
            .clone()
            .or_else(|| app.alias.clone())
            .unwrap_or_else(|| "main".to_string()),
    );

    if let Some(window) = app.context_window_tokens {
        let compact = format_tokens_compact(window);
        if let Some(percent) = app.context_window_percent {
            parts.push(format!("ctx {} @ {}%", compact, percent));
        } else {
            parts.push(format!("ctx {}", compact));
        }
    }

    if let Some(provider) = &app.active_provider_name {
        parts.push(provider.clone());
    }

    parts.join(" | ")
}

fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> usize {
    if width == 0 {
        return lines.len();
    }

    let width = usize::from(width);
    lines
        .iter()
        .map(|line| {
            let line_width = line.width();
            if line_width == 0 {
                1
            } else {
                line_width.div_ceil(width)
            }
        })
        .sum()
}

fn visual_height_for_line(line: &Line<'_>, width: u16) -> usize {
    if width == 0 {
        return 1;
    }

    let width = usize::from(width);
    let line_width = line.width();
    if line_width == 0 {
        1
    } else {
        line_width.div_ceil(width)
    }
}

fn transcript_viewport(
    mut lines: Vec<Line<'static>>,
    width: u16,
    height: u16,
    scroll_back: usize,
) -> TranscriptViewport {
    let viewport_height = usize::from(height.max(1));
    let total_visual_lines = wrapped_line_count(&lines, width);
    let max_scroll_back = total_visual_lines.saturating_sub(viewport_height);
    let clamped_scroll_back = scroll_back.min(max_scroll_back);
    let scroll_top = total_visual_lines
        .saturating_sub(viewport_height)
        .saturating_sub(clamped_scroll_back);
    let required_visual_lines = viewport_height
        .saturating_add(clamped_scroll_back)
        .saturating_add(viewport_height.saturating_mul(TRANSCRIPT_WINDOW_BUFFER_VIEWPORTS))
        .max(MIN_TRANSCRIPT_WINDOW_VISUAL_LINES);

    if total_visual_lines <= required_visual_lines {
        return TranscriptViewport {
            lines,
            scroll_top,
            total_visual_lines,
        };
    }

    let mut kept_visual_lines = 0usize;
    let mut start_index = lines.len();
    for (index, line) in lines.iter().enumerate().rev() {
        kept_visual_lines = kept_visual_lines.saturating_add(visual_height_for_line(line, width));
        start_index = index;
        if kept_visual_lines >= required_visual_lines {
            break;
        }
    }

    let lines = lines.split_off(start_index);
    let window_visual_lines = wrapped_line_count(&lines, width);
    let window_scroll_top = window_visual_lines
        .saturating_sub(viewport_height)
        .saturating_sub(clamped_scroll_back);

    TranscriptViewport {
        lines,
        scroll_top: window_scroll_top,
        total_visual_lines,
    }
}

#[cfg(test)]
fn transcript_scroll_offset(
    lines: &[Line<'_>],
    width: u16,
    height: u16,
    scroll_back: usize,
) -> usize {
    let total_visual_lines = wrapped_line_count(lines, width);
    let viewport_height = usize::from(height.max(1));
    let max_scroll = total_visual_lines.saturating_sub(viewport_height);
    max_scroll.saturating_sub(scroll_back.min(max_scroll))
}

fn clamp_scroll_top(lines: &[Line<'_>], width: u16, height: u16, scroll_top: usize) -> usize {
    let total_visual_lines = wrapped_line_count(lines, width);
    let viewport_height = usize::from(height.max(1));
    let max_scroll = total_visual_lines.saturating_sub(viewport_height);
    scroll_top.min(max_scroll)
}

fn scroll_percent(lines: &[Line<'_>], width: u16, height: u16, scroll_top: usize) -> usize {
    scroll_percent_for_total(
        wrapped_line_count(lines, width),
        usize::from(height.max(1)),
        scroll_top,
    )
}

fn scroll_percent_for_total(
    total_visual_lines: usize,
    viewport_height: usize,
    scroll_top: usize,
) -> usize {
    let max_scroll = total_visual_lines.saturating_sub(viewport_height);
    if max_scroll == 0 {
        100
    } else {
        (((scroll_top.min(max_scroll)) as f64 / max_scroll as f64) * 100.0).round() as usize
    }
}

fn bottom_padding_line_count(lines: &[Line<'_>], width: u16, height: u16) -> usize {
    let total_visual_lines = wrapped_line_count(lines, width);
    usize::from(height).saturating_sub(total_visual_lines)
}

fn picker_visible_range(total: usize, selected: usize, height: u16) -> (usize, usize) {
    let visible = usize::from(height.saturating_sub(2)).max(1);
    if total <= visible {
        return (0, total);
    }

    let mut start = selected.saturating_sub(visible / 2);
    if start + visible > total {
        start = total.saturating_sub(visible);
    }
    (start, (start + visible).min(total))
}

fn composer_cursor_visual_offset(app: &TuiApp<'_>, width: u16) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }

    let max_width = usize::from(width);
    let mut row = 0usize;
    let mut column = 2usize;

    for ch in app.input[..app.input_cursor].chars() {
        if ch == '\n' {
            row += 1;
            column = 2;
            continue;
        }

        if column >= max_width {
            row += 1;
            column = 2;
        }

        column += 1;
        if column >= max_width {
            row += 1;
            column = 2;
        }
    }

    (row, column)
}

fn composer_cursor_position(app: &TuiApp<'_>, area: Rect) -> Option<Position> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let (line_index, column) = composer_cursor_visual_offset(app, area.width);

    Some(Position::new(
        area.x
            .saturating_add(u16::try_from(column).unwrap_or(u16::MAX))
            .min(area.x.saturating_add(area.width.saturating_sub(1))),
        area.y
            .saturating_add(u16::try_from(line_index).unwrap_or(u16::MAX))
            .min(area.y.saturating_add(area.height.saturating_sub(1))),
    ))
}

fn fmt_elapsed(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        return format!("{elapsed_secs}s");
    }
    if elapsed_secs < 3600 {
        let minutes = elapsed_secs / 60;
        let seconds = elapsed_secs % 60;
        return format!("{minutes}m {seconds:02}s");
    }
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    format!("{hours}h {minutes:02}m {seconds:02}s")
}

fn format_tokens_compact(value: i64) -> String {
    let value = value.max(0);
    if value == 0 {
        return "0".to_string();
    }
    if value < 1_000 {
        return value.to_string();
    }

    let value_f64 = value as f64;
    let (scaled, suffix) = if value >= 1_000_000_000_000 {
        (value_f64 / 1_000_000_000_000.0, "T")
    } else if value >= 1_000_000_000 {
        (value_f64 / 1_000_000_000.0, "B")
    } else if value >= 1_000_000 {
        (value_f64 / 1_000_000.0, "M")
    } else {
        (value_f64 / 1_000.0, "K")
    };

    let decimals = if scaled < 10.0 {
        2
    } else if scaled < 100.0 {
        1
    } else {
        0
    };

    let mut formatted = format!("{scaled:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    format!("{formatted}{suffix}")
}

pub(super) fn help_text() -> &'static str {
    "/help\n/status\n/config\n/dashboard\n/telegrams\n/discords\n/slacks\n/signals\n/home-assistant\n/telegram approvals\n/discord approvals\n/slack approvals\n/webhooks\n/inboxes\n/autopilot [on|pause|resume|status]\n/missions\n/events [limit]\n/schedule <seconds> <title>\n/repeat <seconds> <title>\n/watch <path> <title>\n/profile\n/memory [query]\n/remember <text>\n/forget <memory-id>\n/skills [drafts|published|rejected]\n/skills publish <draft-id>\n/skills reject <draft-id>\n/model [name]\n/permissions [preset]\n/attach <path>\n/attachments\n/detach\n/thinking [level]\n/fast\n/review [instructions]\n/compact\n/resume\n/fork\n/rename <title>\n/new\n/clear\n!<command>\n/exit\n\nMain view:\n  Enter send\n  Ctrl+J or Shift+Enter newline\n  Ctrl+T transcript overlay\n  Up/Down scroll transcript when composer is empty\n  PageUp/PageDown jump transcript\n  Ctrl+A / Ctrl+E line start/end\n\nSettings:\n  /config opens a simple settings home with categories\n  /dashboard opens the localhost web control room\n\nOverlays:\n  Esc or q close\n  Up/Down or j/k scroll\n  PageUp/PageDown jump\n  Home/End top or bottom\n\nPickers:\n  Type to filter\n  Up/Down move selection\n  Enter select\n  Esc cancel\n  PageUp/PageDown jump\n  Mouse wheel scroll"
}

fn centered_rect(horizontal: u16, vertical: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - vertical) / 2),
            Constraint::Percentage(vertical),
            Constraint::Percentage((100 - vertical) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - horizontal) / 2),
            Constraint::Percentage(horizontal),
            Constraint::Percentage((100 - horizontal) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::bottom_padding_line_count;
    use super::picker_visible_range;
    use super::scroll_percent_for_total;
    use super::transcript_scroll_offset;
    use super::transcript_viewport;
    use super::visual_height_for_line;
    use super::wrapped_line_count;
    use ratatui::text::Line;

    #[test]
    fn wrapped_line_count_counts_visual_wrap_height() {
        let lines = vec![Line::from("1234567890"), Line::from("abc"), Line::from("")];

        assert_eq!(wrapped_line_count(&lines, 5), 4);
    }

    #[test]
    fn wrapped_line_count_handles_zero_width() {
        let lines = vec![Line::from("1234567890"), Line::from("abc")];

        assert_eq!(wrapped_line_count(&lines, 0), 2);
    }

    #[test]
    fn visual_height_handles_empty_lines() {
        assert_eq!(visual_height_for_line(&Line::from(""), 10), 1);
    }

    #[test]
    fn transcript_scroll_offset_tracks_manual_scroll_back_from_bottom() {
        let lines = vec![
            Line::from("1"),
            Line::from("2"),
            Line::from("3"),
            Line::from("4"),
            Line::from("5"),
        ];

        assert_eq!(transcript_scroll_offset(&lines, 10, 3, 0), 2);
        assert_eq!(transcript_scroll_offset(&lines, 10, 3, 1), 1);
        assert_eq!(transcript_scroll_offset(&lines, 10, 3, usize::MAX), 0);
    }

    #[test]
    fn bottom_padding_line_count_bottom_anchors_short_transcripts() {
        let lines = vec![Line::from("hello"), Line::from("world")];

        assert_eq!(bottom_padding_line_count(&lines, 10, 5), 3);
    }

    #[test]
    fn transcript_viewport_expands_with_scroll_back_instead_of_stopping_at_fixed_window() {
        let lines = (0..600)
            .map(|index| Line::from(format!("line {index}")))
            .collect::<Vec<_>>();

        let viewport = transcript_viewport(lines, 40, 10, 300);

        assert!(viewport.lines.len() >= 350);
        assert_eq!(viewport.total_visual_lines, 600);
        assert_eq!(viewport.scroll_top, 90);
    }

    #[test]
    fn scroll_percent_uses_total_transcript_height() {
        assert_eq!(scroll_percent_for_total(100, 10, 90), 100);
        assert_eq!(scroll_percent_for_total(100, 10, 0), 0);
    }

    #[test]
    fn picker_visible_range_tracks_selected_row() {
        assert_eq!(picker_visible_range(20, 0, 8), (0, 6));
        assert_eq!(picker_visible_range(20, 10, 8), (7, 13));
        assert_eq!(picker_visible_range(20, 19, 8), (14, 20));
    }
}
