#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import phase2_smoke as common


OPENAI_PROVIDER_ID = "openai"
MOONSHOT_PROVIDER_ID = "moonshot"
OPENROUTER_PROVIDER_ID = "openrouter"
VENICE_PROVIDER_ID = "venice"
ANTHROPIC_PROVIDER_ID = "anthropic"
OLLAMA_PROVIDER_ID = "ollama-local"
CODEX_PROVIDER_ID = "codex-browser"

OPENAI_MODEL = "gpt-phase2"
ANTHROPIC_MODEL = "claude-phase2"
OLLAMA_MODEL = "llama-phase2"
CODEX_MODEL = "gpt-5-codex"

OPENAI_MAIN_ALIAS = "main"
MOONSHOT_ALIAS = "moonshot"
OPENROUTER_ALIAS = "openrouter"
VENICE_ALIAS = "venice"
ANTHROPIC_ALIAS = "anthropic"
OLLAMA_ALIAS = "ollama"
CODEX_ALIAS = "codex"

WEBHOOK_ID = "phase2-webhook"
TELEGRAM_ID = "phase2-telegram"
DISCORD_ID = "phase2-discord"
SLACK_ID = "phase2-slack"
SIGNAL_ID = "phase2-signal"
HOME_ASSISTANT_ID = "phase2-home"
GMAIL_ID = "phase2-gmail"
BRAVE_ID = "phase2-brave"
BRAVE_ALT_ID = "phase2-brave-alt"

TELEGRAM_TOKEN = "telegram-token"
DISCORD_TOKEN = "discord-token"
SLACK_TOKEN = "slack-token"
GMAIL_TOKEN = "gmail-token"
HOME_ASSISTANT_TOKEN = "home-token"
BRAVE_TOKEN = "brave-token"
OPENAI_TOKEN = "openai-key"
MOONSHOT_TOKEN = "moonshot-key"
OPENROUTER_TOKEN = "openrouter-key"
VENICE_TOKEN = "venice-key"
ANTHROPIC_TOKEN = "anthropic-key"
CODEX_TOKEN = "codex-token"

TELEGRAM_CHAT_ID = 444001
TELEGRAM_USER_ID = 555001
DISCORD_CHANNEL_ID = "phase2-discord-channel"
DISCORD_USER_ID = "phase2-discord-user"
SLACK_CHANNEL_ID = "C_PHASE2"
SLACK_USER_ID = "U_PHASE2"
SIGNAL_GROUP_ID = "phase2-signal-group"
SIGNAL_USER_ID = "+15551234567"
GMAIL_SENDER = "ops@example.com"
HOME_ASSISTANT_ENTITY_ID = "light.office"
WEBHOOK_TOKEN = "phase2-webhook-token"

SIGNAL_ACCOUNT = "+15550001111"
SIGNAL_MESSAGE_TEXT = "Phase 2 signal request"
BRAVE_PROMPT = "Use Brave search for the Phase 2 brave validation query."


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Phase 2 provider, connector, approvals, and delegation certification matrix"
    )
    parser.add_argument("--binary-path", required=True)
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--scenario-root", required=True)
    parser.add_argument("--daemon-port", type=int, default=42921)
    parser.add_argument("--mock-port", type=int, default=42922)
    return parser.parse_args()


class MockServiceState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.telegram_next_message_id = 9100
        self.telegram_updates = [
            {
                "update_id": 1001,
                "message": {
                    "message_id": 2001,
                    "date": 1716000001,
                    "chat": {
                        "id": TELEGRAM_CHAT_ID,
                        "type": "private",
                        "first_name": "Phase",
                        "last_name": "Operator",
                        "username": "phase2",
                    },
                    "from": {
                        "id": TELEGRAM_USER_ID,
                        "is_bot": False,
                        "first_name": "Phase",
                        "last_name": "Operator",
                        "username": "phase2user",
                    },
                    "text": "Phase 2 telegram request",
                },
            }
        ]
        self.telegram_sent: list[dict[str, Any]] = []

        self.discord_messages = {
            DISCORD_CHANNEL_ID: [
                {
                    "id": "3001",
                    "channel_id": DISCORD_CHANNEL_ID,
                    "channel_name": "phase2-ops",
                    "guild_id": "guild-1",
                    "content": "Phase 2 discord request",
                    "timestamp": "2026-03-28T12:00:00.000Z",
                    "attachments": [],
                    "author": {
                        "id": DISCORD_USER_ID,
                        "username": "phase2",
                        "global_name": "Phase Operator",
                        "bot": False,
                    },
                }
            ]
        }
        self.discord_sent: list[dict[str, Any]] = []

        self.slack_messages = {
            SLACK_CHANNEL_ID: [
                {
                    "type": "message",
                    "text": "Phase 2 slack request",
                    "ts": "1716000001.000100",
                    "user": SLACK_USER_ID,
                    "channel": SLACK_CHANNEL_ID,
                }
            ]
        }
        self.slack_sent: list[dict[str, Any]] = []

        self.gmail_messages = [
            {
                "id": "gmail-1",
                "threadId": "gmail-thread-1",
                "from": GMAIL_SENDER,
                "subject": "Phase 2 Gmail request",
                "snippet": "Phase 2 gmail body",
            }
        ]
        self.gmail_sent: list[dict[str, Any]] = []

        self.home_entities: dict[str, dict[str, Any]] = {
            HOME_ASSISTANT_ENTITY_ID: {
                "entity_id": HOME_ASSISTANT_ENTITY_ID,
                "state": "off",
                "friendly_name": "Office Light",
                "last_changed": "2026-03-28T12:00:00Z",
                "last_updated": "2026-03-28T12:00:00Z",
                "attributes": {"friendly_name": "Office Light"},
            }
        }
        self.home_service_calls: list[dict[str, Any]] = []
        self.brave_queries: list[str] = []

    def set_home_entity_state(self, entity_id: str, state: str, marker: str) -> None:
        with self.lock:
            entity = self.home_entities.setdefault(
                entity_id,
                {
                    "entity_id": entity_id,
                    "friendly_name": entity_id,
                    "attributes": {},
                },
            )
            entity["state"] = state
            entity["last_changed"] = marker
            entity["last_updated"] = marker
            attributes = dict(entity.get("attributes") or {})
            if entity.get("friendly_name"):
                attributes.setdefault("friendly_name", entity["friendly_name"])
            entity["attributes"] = attributes


class MockServiceHandler(BaseHTTPRequestHandler):
    server_version = "Phase2MatrixMock/1.0"

    @property
    def state(self) -> MockServiceState:
        return self.server.state  # type: ignore[attr-defined]

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def _read_json_body(self) -> Any:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        return json.loads(raw or b"{}")

    def _send_json(self, status: int, payload: Any, headers: dict[str, str] | None = None) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for key, value in (headers or {}).items():
            self.send_header(key, value)
        self.end_headers()
        self.wfile.write(body)

    def _send_sse(self, events: list[tuple[str, dict[str, Any]]]) -> None:
        chunks = []
        for event_name, payload in events:
            chunks.append(f"event: {event_name}\n")
            chunks.append(f"data: {json.dumps(payload)}\n\n")
        body = "".join(chunks).encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_text(self, status: int, text: str) -> None:
        body = text.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _authorization_token(self) -> str | None:
        value = self.headers.get("Authorization", "").strip()
        if not value.lower().startswith("bearer "):
            return None
        return value.split(" ", 1)[1].strip()

    def _require_openai_auth(self) -> bool:
        token = self._authorization_token()
        if token is None:
            return True
        if token in {OPENAI_TOKEN, MOONSHOT_TOKEN, OPENROUTER_TOKEN, VENICE_TOKEN}:
            return True
        self._send_json(
            HTTPStatus.UNAUTHORIZED,
            {"error": {"message": "missing or invalid API key"}},
        )
        return False

    def _require_anthropic_auth(self) -> bool:
        token = self.headers.get("x-api-key", "").strip()
        if token == ANTHROPIC_TOKEN:
            return True
        self._send_json(
            HTTPStatus.UNAUTHORIZED,
            {"error": {"message": "invalid x-api-key"}},
        )
        return False

    def _require_gmail_auth(self) -> bool:
        token = self._authorization_token()
        if token == GMAIL_TOKEN:
            return True
        self._send_json(
            HTTPStatus.UNAUTHORIZED,
            {"error": {"message": "invalid gmail token"}},
        )
        return False

    def _require_codex_auth(self) -> bool:
        token = self._authorization_token()
        if token == CODEX_TOKEN:
            return True
        self._send_json(
            HTTPStatus.UNAUTHORIZED,
            {"error": {"message": "authentication token invalid"}},
        )
        return False

    def _extract_openai_text(self, value: Any) -> str:
        if isinstance(value, str):
            return value
        if isinstance(value, list):
            parts = []
            for item in value:
                if isinstance(item, dict) and item.get("type") == "text":
                    parts.append(str(item.get("text", "")))
            return "\n".join(parts)
        return ""

    def _last_openai_user_text(self, messages: list[dict[str, Any]]) -> str:
        for message in reversed(messages):
            if message.get("role") == "user":
                return self._extract_openai_text(message.get("content"))
        return "empty"

    def _last_openai_tool_content(self, messages: list[dict[str, Any]]) -> str | None:
        for message in reversed(messages):
            if message.get("role") == "tool":
                content = message.get("content")
                return content if isinstance(content, str) else json.dumps(content)
        return None

    def _last_anthropic_user_text(self, messages: list[dict[str, Any]]) -> str:
        for message in reversed(messages):
            if message.get("role") != "user":
                continue
            content = message.get("content")
            if isinstance(content, str):
                return content
            if isinstance(content, list):
                parts = []
                for item in content:
                    if isinstance(item, dict) and item.get("type") == "text":
                        parts.append(str(item.get("text", "")))
                if parts:
                    return "\n".join(parts)
        return "empty"

    def _last_codex_user_text(self, items: list[dict[str, Any]]) -> str:
        for item in reversed(items):
            if item.get("type") != "message" or item.get("role") not in {"user", "developer"}:
                continue
            content = item.get("content")
            if not isinstance(content, list):
                continue
            parts = []
            for block in content:
                if isinstance(block, dict) and block.get("type") == "input_text":
                    parts.append(str(block.get("text", "")))
            if parts:
                return "\n".join(parts)
        return "empty"

    def _last_codex_tool_output(self, items: list[dict[str, Any]]) -> str | None:
        for item in reversed(items):
            if item.get("type") == "function_call_output":
                output = item.get("output")
                if isinstance(output, str):
                    return output
                return json.dumps(output)
        return None

    def do_GET(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlsplit(self.path)
        path = parsed.path
        query = urllib.parse.parse_qs(parsed.query)

        if path == "/openai/v1/models":
            if not self._require_openai_auth():
                return
            self._send_json(
                HTTPStatus.OK,
                {"data": [{"id": OPENAI_MODEL}, {"id": "gpt-phase2-alt"}]},
            )
            return

        if path == "/anthropic/v1/models":
            if not self._require_anthropic_auth():
                return
            self._send_json(HTTPStatus.OK, {"data": [{"id": ANTHROPIC_MODEL}]})
            return

        if path == "/ollama/api/tags":
            self._send_json(HTTPStatus.OK, {"models": [{"name": OLLAMA_MODEL}]})
            return

        if path == "/codex/models":
            if not self._require_codex_auth():
                return
            self._send_json(
                HTTPStatus.OK,
                {
                    "models": [
                        {
                            "slug": CODEX_MODEL,
                            "display_name": "GPT-5 Codex",
                            "visibility": "public",
                            "supports_parallel_tool_calls": True,
                        }
                    ]
                },
            )
            return

        if path.startswith("/telegram/bot") and path.endswith("/getUpdates"):
            token = path.split("/telegram/bot", 1)[1].split("/", 1)[0]
            if token != TELEGRAM_TOKEN:
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"ok": False, "description": "invalid telegram token"},
                )
                return
            offset = int(query.get("offset", ["0"])[0])
            with self.state.lock:
                updates = [
                    update
                    for update in self.state.telegram_updates
                    if update["update_id"] >= max(offset, 1)
                ]
            self._send_json(HTTPStatus.OK, {"ok": True, "result": updates})
            return

        if path.startswith("/discord/api/v10/channels/") and path.endswith("/messages"):
            if self.headers.get("Authorization", "").strip() != f"Bot {DISCORD_TOKEN}":
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"message": "invalid discord token"},
                )
                return
            channel_id = path.split("/channels/", 1)[1].split("/messages", 1)[0]
            after = query.get("after", [None])[0]
            with self.state.lock:
                messages = list(self.state.discord_messages.get(channel_id, []))
            if after:
                messages = [message for message in messages if int(message["id"]) > int(after)]
            self._send_json(HTTPStatus.OK, messages)
            return

        if path == "/slack/api/conversations.history":
            auth = self.headers.get("Authorization", "").strip()
            if auth != f"Bearer {SLACK_TOKEN}":
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"ok": False, "error": "invalid_auth"},
                )
                return
            channel_id = query.get("channel", [""])[0]
            oldest = query.get("oldest", [None])[0]
            with self.state.lock:
                messages = list(self.state.slack_messages.get(channel_id, []))
            if oldest:
                oldest_value = float(oldest)
                messages = [
                    message for message in messages if float(message["ts"]) > oldest_value
                ]
            self._send_json(HTTPStatus.OK, {"ok": True, "messages": messages})
            return

        if path == "/gmail/gmail/v1/users/me/messages":
            if not self._require_gmail_auth():
                return
            with self.state.lock:
                messages = [
                    {"id": item["id"], "threadId": item["threadId"]}
                    for item in self.state.gmail_messages
                ]
            self._send_json(HTTPStatus.OK, {"messages": messages})
            return

        if path.startswith("/gmail/gmail/v1/users/me/messages/"):
            if not self._require_gmail_auth():
                return
            message_id = path.rsplit("/", 1)[-1]
            with self.state.lock:
                message = next(
                    (item for item in self.state.gmail_messages if item["id"] == message_id),
                    None,
                )
            if message is None:
                self._send_json(HTTPStatus.NOT_FOUND, {"error": {"message": "not found"}})
                return
            self._send_json(
                HTTPStatus.OK,
                {
                    "id": message["id"],
                    "threadId": message["threadId"],
                    "snippet": message["snippet"],
                    "payload": {
                        "headers": [
                            {"name": "From", "value": message["from"]},
                            {"name": "Subject", "value": message["subject"]},
                        ]
                    },
                },
            )
            return

        if path.startswith("/home-assistant/api/states/"):
            if self._authorization_token() != HOME_ASSISTANT_TOKEN:
                self._send_text(HTTPStatus.UNAUTHORIZED, "invalid home assistant token")
                return
            entity_id = path.split("/api/states/", 1)[1]
            with self.state.lock:
                entity = self.state.home_entities.get(entity_id)
            if entity is None:
                self._send_text(HTTPStatus.NOT_FOUND, "unknown entity")
                return
            self._send_json(HTTPStatus.OK, entity)
            return

        if path.startswith("/brave/res/v1/web/search"):
            token = self.headers.get("X-Subscription-Token", "").strip()
            if token != BRAVE_TOKEN:
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"error": {"message": "invalid brave api key"}},
                )
                return
            query_text = query.get("q", [""])[0]
            with self.state.lock:
                self.state.brave_queries.append(query_text)
            self._send_json(
                HTTPStatus.OK,
                {
                    "web": {
                        "more_results_available": False,
                        "results": [
                            {
                                "title": "Phase 2 Brave Result",
                                "url": "https://example.com/phase2",
                                "description": f"Brave hit for {query_text}",
                            }
                        ],
                    }
                },
            )
            return

        self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlsplit(self.path)
        path = parsed.path

        if path == "/openai/v1/chat/completions":
            if not self._require_openai_auth():
                return
            payload = self._read_json_body()
            model = payload.get("model") or OPENAI_MODEL
            if model not in {OPENAI_MODEL, "gpt-phase2-alt"}:
                self._send_json(
                    HTTPStatus.BAD_REQUEST,
                    {"error": {"message": f"model '{model}' not available"}},
                )
                return
            messages = payload.get("messages") or []
            tool_output = self._last_openai_tool_content(messages)
            if tool_output is not None:
                self._send_json(
                    HTTPStatus.OK,
                    {
                        "id": "chatcmpl-phase2-tool",
                        "object": "chat.completion",
                        "created": int(time.time()),
                        "model": model,
                        "choices": [
                            {
                                "index": 0,
                                "message": {
                                    "role": "assistant",
                                    "content": f"Tool result: {tool_output}",
                                },
                                "finish_reason": "stop",
                            }
                        ],
                    },
                )
                return
            prompt = self._last_openai_user_text(messages)
            tools = payload.get("tools") or []
            if tools and "brave validation" in prompt.lower():
                self._send_json(
                    HTTPStatus.OK,
                    {
                        "id": "chatcmpl-phase2-brave",
                        "object": "chat.completion",
                        "created": int(time.time()),
                        "model": model,
                        "choices": [
                            {
                                "index": 0,
                                "message": {
                                    "role": "assistant",
                                    "content": None,
                                    "tool_calls": [
                                        {
                                            "id": "call_phase2_brave",
                                            "type": "function",
                                            "function": {
                                                "name": "brave_web_search",
                                                "arguments": json.dumps(
                                                    {
                                                        "query": "phase 2 brave validation",
                                                        "count": 1,
                                                    }
                                                ),
                                            },
                                        }
                                    ],
                                },
                                "finish_reason": "tool_calls",
                            }
                        ],
                    },
                )
                return
            self._send_json(
                HTTPStatus.OK,
                {
                    "id": "chatcmpl-phase2",
                    "object": "chat.completion",
                    "created": int(time.time()),
                    "model": model,
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": f"OpenAI-family reply from {model}: {prompt}",
                            },
                            "finish_reason": "stop",
                        }
                    ],
                },
            )
            return

        if path == "/anthropic/v1/messages":
            if not self._require_anthropic_auth():
                return
            payload = self._read_json_body()
            model = payload.get("model") or ANTHROPIC_MODEL
            if model != ANTHROPIC_MODEL:
                self._send_json(
                    HTTPStatus.BAD_REQUEST,
                    {"error": {"message": f"model '{model}' not available"}},
                )
                return
            prompt = self._last_anthropic_user_text(payload.get("messages") or [])
            self._send_json(
                HTTPStatus.OK,
                {
                    "id": "msg_phase2",
                    "content": [
                        {
                            "type": "text",
                            "text": f"Anthropic reply from {model}: {prompt}",
                        }
                    ],
                },
            )
            return

        if path == "/ollama/api/chat":
            payload = self._read_json_body()
            model = payload.get("model") or OLLAMA_MODEL
            if model != OLLAMA_MODEL:
                self._send_json(HTTPStatus.BAD_REQUEST, {"error": "model not found"})
                return
            prompt = ""
            for message in reversed(payload.get("messages") or []):
                if message.get("role") == "user":
                    prompt = str(message.get("content", ""))
                    break
            self._send_json(
                HTTPStatus.OK,
                {"message": {"content": f"Ollama reply from {model}: {prompt}"}},
            )
            return

        if path == "/codex/responses":
            if not self._require_codex_auth():
                return
            payload = self._read_json_body()
            model = payload.get("model") or CODEX_MODEL
            if model != CODEX_MODEL:
                self._send_json(
                    HTTPStatus.BAD_REQUEST,
                    {"error": {"message": f"model '{model}' not available"}},
                )
                return
            input_items = payload.get("input") or []
            tool_output = self._last_codex_tool_output(input_items)
            if tool_output is not None:
                self._send_sse(
                    [
                        ("response.output_text.delta", {"delta": f"Codex tool result: {tool_output}"}),
                        ("response.completed", {"response": {"id": "resp_phase2_tool"}}),
                    ]
                )
                return
            prompt = self._last_codex_user_text(input_items)
            self._send_sse(
                [
                    ("response.output_text.delta", {"delta": f"Codex reply from {model}: {prompt}"}),
                    ("response.completed", {"response": {"id": "resp_phase2"}}),
                ]
            )
            return

        if path.startswith("/telegram/bot") and path.endswith("/sendMessage"):
            token = path.split("/telegram/bot", 1)[1].split("/", 1)[0]
            if token != TELEGRAM_TOKEN:
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"ok": False, "description": "invalid telegram token"},
                )
                return
            payload = self._read_json_body()
            with self.state.lock:
                self.state.telegram_next_message_id += 1
                message_id = self.state.telegram_next_message_id
                self.state.telegram_sent.append(
                    {
                        "chat_id": payload.get("chat_id"),
                        "text": payload.get("text"),
                        "disable_notification": payload.get("disable_notification"),
                    }
                )
            self._send_json(
                HTTPStatus.OK,
                {"ok": True, "result": {"message_id": message_id}},
            )
            return

        if path.startswith("/discord/api/v10/channels/") and path.endswith("/messages"):
            if self.headers.get("Authorization", "").strip() != f"Bot {DISCORD_TOKEN}":
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"message": "invalid discord token"},
                )
                return
            channel_id = path.split("/channels/", 1)[1].split("/messages", 1)[0]
            payload = self._read_json_body()
            with self.state.lock:
                message_id = str(4000 + len(self.state.discord_sent) + 1)
                self.state.discord_sent.append(
                    {
                        "channel_id": channel_id,
                        "content": payload.get("content"),
                    }
                )
            self._send_json(HTTPStatus.OK, {"id": message_id, "channel_id": channel_id})
            return

        if path == "/slack/api/chat.postMessage":
            auth = self.headers.get("Authorization", "").strip()
            if auth != f"Bearer {SLACK_TOKEN}":
                self._send_json(
                    HTTPStatus.UNAUTHORIZED,
                    {"ok": False, "error": "invalid_auth"},
                )
                return
            payload = self._read_json_body()
            with self.state.lock:
                ts = f"1716000100.000{len(self.state.slack_sent) + 1}"
                self.state.slack_sent.append(
                    {
                        "channel": payload.get("channel"),
                        "text": payload.get("text"),
                    }
                )
            self._send_json(
                HTTPStatus.OK,
                {"ok": True, "channel": payload.get("channel"), "ts": ts},
            )
            return

        if path == "/gmail/gmail/v1/users/me/messages/send":
            if not self._require_gmail_auth():
                return
            payload = self._read_json_body()
            with self.state.lock:
                message_id = f"gmail-sent-{len(self.state.gmail_sent) + 1}"
                self.state.gmail_sent.append(payload)
            self._send_json(HTTPStatus.OK, {"id": message_id})
            return

        if path.startswith("/home-assistant/api/services/"):
            if self._authorization_token() != HOME_ASSISTANT_TOKEN:
                self._send_text(HTTPStatus.UNAUTHORIZED, "invalid home assistant token")
                return
            payload = self._read_json_body()
            suffix = path.split("/api/services/", 1)[1]
            domain, service = suffix.split("/", 1)
            entity_id = payload.get("entity_id")
            with self.state.lock:
                self.state.home_service_calls.append(
                    {
                        "domain": domain,
                        "service": service,
                        "entity_id": entity_id,
                        "payload": payload,
                    }
                )
            if entity_id and service == "turn_on":
                self.state.set_home_entity_state(
                    entity_id,
                    "on",
                    "2026-03-28T12:10:00Z",
                )
            self._send_json(
                HTTPStatus.OK,
                [{"entity_id": entity_id or HOME_ASSISTANT_ENTITY_ID}],
            )
            return

        self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})


class MockServiceServer:
    def __init__(self, host: str, port: int) -> None:
        self.state = MockServiceState()
        self._server = ThreadingHTTPServer((host, port), MockServiceHandler)
        self._server.state = self.state  # type: ignore[attr-defined]
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)


def http_request_json(
    method: str,
    url: str,
    *,
    body: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    expected_status: int | None = HTTPStatus.OK,
) -> Any:
    data = None
    request_headers = dict(headers or {})
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        request_headers.setdefault("Content-Type", "application/json")
    request = urllib.request.Request(url, data=data, headers=request_headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status = response.status
            payload = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        status = error.code
        payload = error.read().decode("utf-8")
    if expected_status is not None and status != expected_status:
        raise RuntimeError(f"unexpected status for {method} {url}: {status}\n{payload}")
    if not payload:
        return None
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return payload


def daemon_json(
    base_url: str,
    auth_headers: dict[str, str],
    method: str,
    path: str,
    *,
    body: dict[str, Any] | None = None,
    expected_status: int | None = HTTPStatus.OK,
) -> Any:
    return http_request_json(
        method,
        f"{base_url}{path}",
        body=body,
        headers=auth_headers,
        expected_status=expected_status,
    )


def command_failure_output(result: Any) -> str:
    return f"{result.stdout or ''}\n{result.stderr or ''}"


def command_json(
    binary_path: Path,
    env: dict[str, str],
    cwd: Path,
    *args: str,
) -> Any:
    result = common.run_command([str(binary_path), *args], env=env, cwd=cwd)
    return json.loads(result.stdout)


def last_exec_event(
    binary_path: Path,
    env: dict[str, str],
    cwd: Path,
    alias: str,
    prompt: str,
    *,
    permissions: str | None = None,
) -> dict[str, Any]:
    command = [str(binary_path), "exec", "--json", "--alias", alias, "--mode", "build"]
    if permissions:
        command.extend(["--permissions", permissions])
    command.append(prompt)
    result = common.run_command(command, env=env, cwd=cwd)
    return json.loads(result.stdout.strip().splitlines()[-1])


def wait_for_cli_json(
    binary_path: Path,
    env: dict[str, str],
    cwd: Path,
    *args: str,
    predicate: Any,
    description: str,
    timeout: float = 20.0,
) -> Any:
    deadline = time.time() + timeout
    last_payload: Any = None
    while time.time() < deadline:
        last_payload = command_json(binary_path, env, cwd, *args)
        if predicate(last_payload):
            return last_payload
        time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for {description}: {last_payload}")


def wait_for_daemon_json(
    base_url: str,
    auth_headers: dict[str, str],
    path: str,
    *,
    predicate: Any,
    description: str,
    timeout: float = 20.0,
) -> Any:
    deadline = time.time() + timeout
    last_payload: Any = None
    while time.time() < deadline:
        last_payload = daemon_json(base_url, auth_headers, "GET", path)
        if predicate(last_payload):
            return last_payload
        time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for {description}: {last_payload}")


def wait_for_cli_output_contains(
    binary_path: Path,
    env: dict[str, str],
    cwd: Path,
    args: list[str],
    needle: str,
    *,
    description: str,
    timeout: float = 20.0,
) -> str:
    deadline = time.time() + timeout
    last_output = ""
    while time.time() < deadline:
        result = common.run_command([str(binary_path), *args], env=env, cwd=cwd)
        last_output = result.stdout
        if needle in last_output:
            return last_output
        time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for {description}: {last_output}")


def add_alias(
    binary_path: Path,
    env: dict[str, str],
    cwd: Path,
    alias: str,
    provider: str,
    model: str,
    *,
    main: bool = False,
) -> None:
    args = [
        str(binary_path),
        "alias",
        "add",
        "--alias",
        alias,
        "--provider",
        provider,
        "--model",
        model,
    ]
    if main:
        args.append("--main")
    common.run_command(args, env=env, cwd=cwd)


def build_codex_provider_request(provider_id: str, base_url: str, access_token: str) -> dict[str, Any]:
    return {
        "provider": {
            "id": provider_id,
            "display_name": provider_id,
            "kind": "chat_gpt_codex",
            "base_url": base_url,
            "auth_mode": "o_auth",
            "default_model": CODEX_MODEL,
            "keychain_account": None,
            "oauth": {
                "client_id": "phase2",
                "authorization_url": "http://127.0.0.1/authorize",
                "token_url": "http://127.0.0.1/token",
                "scopes": [],
                "extra_authorize_params": [],
                "extra_token_params": [],
            },
            "local": False,
        },
        "api_key": None,
        "oauth_token": {
            "access_token": access_token,
            "refresh_token": None,
            "expires_at": None,
            "token_type": "Bearer",
            "scopes": [],
            "id_token": None,
            "account_id": "acct-phase2",
            "user_id": None,
            "org_id": None,
            "project_id": None,
            "display_email": None,
            "subscription_type": "team",
        },
    }


def build_ollama_provider_request(provider_id: str, base_url: str, model: str) -> dict[str, Any]:
    return {
        "provider": {
            "id": provider_id,
            "display_name": provider_id,
            "kind": "ollama",
            "base_url": base_url,
            "auth_mode": "none",
            "default_model": model,
            "keychain_account": None,
            "oauth": None,
            "local": True,
        },
        "api_key": None,
        "oauth_token": None,
    }


def write_signal_cli_fixtures(fixtures_root: Path) -> Path:
    signal_root = fixtures_root / "signal-cli"
    signal_root.mkdir(parents=True, exist_ok=True)
    state_dir = signal_root / "state"
    state_dir.mkdir(parents=True, exist_ok=True)
    receive_file = state_dir / "receive.jsonl"
    receive_file.write_text(
        json.dumps(
            {
                "envelope": {
                    "source": SIGNAL_USER_ID,
                    "sourceName": "Phase Operator",
                    "timestamp": 1716000001,
                    "dataMessage": {
                        "message": SIGNAL_MESSAGE_TEXT,
                        "groupInfo": {
                            "groupId": SIGNAL_GROUP_ID,
                            "title": "Phase 2 Group",
                        },
                    },
                }
            }
        )
        + "\n",
        encoding="utf-8",
    )
    (state_dir / "sent.jsonl").write_text("", encoding="utf-8")

    script_path = signal_root / "fake_signal_cli.py"
    script_path.write_text(
        """#!/usr/bin/env python3
from __future__ import annotations
import json
import os
import sys
from pathlib import Path

state_root = Path(os.environ["PHASE2_SIGNAL_STATE"])
receive_path = state_root / "receive.jsonl"
sent_path = state_root / "sent.jsonl"
args = sys.argv[1:]

if "receive" in args:
    sys.stdout.write(receive_path.read_text(encoding="utf-8"))
    receive_path.write_text("", encoding="utf-8")
    raise SystemExit(0)

if "send" in args:
    try:
        message = args[args.index("-m") + 1]
    except Exception:
        raise SystemExit("missing -m payload")
    group_id = None
    recipient = None
    if "-g" in args:
        group_id = args[args.index("-g") + 1]
    else:
        recipient = args[-1]
    with sent_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps({"message": message, "group_id": group_id, "recipient": recipient}) + "\\n")
    raise SystemExit(0)

raise SystemExit("unsupported fake signal-cli invocation")
""",
        encoding="utf-8",
    )

    if os.name == "nt":
        wrapper_path = signal_root / "signal-cli.cmd"
        wrapper_path.write_text(
            "@echo off\r\n"
            f"\"{sys.executable}\" \"%~dp0fake_signal_cli.py\" %*\r\n",
            encoding="utf-8",
        )
    else:
        wrapper_path = signal_root / "signal-cli"
        wrapper_path.write_text(
            "#!/usr/bin/env sh\n"
            f"exec \"{sys.executable}\" \"$(dirname \"$0\")/fake_signal_cli.py\" \"$@\"\n",
            encoding="utf-8",
        )
        wrapper_path.chmod(0o755)
        script_path.chmod(0o755)

    return wrapper_path


def main() -> int:
    args = parse_args()
    binary_path = Path(args.binary_path).resolve()
    repo_root = Path(args.repo_root).resolve()
    scenario_root = Path(args.scenario_root).resolve()
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)

    common.LOG_PATH = scenario_root / "phase2-matrix.log"
    common.LOG_PATH.write_text("", encoding="utf-8")

    env = common.configure_profile_env(scenario_root)
    base_url = f"http://127.0.0.1:{args.daemon_port}"
    status_url = f"{base_url}/v1/status"
    auth_headers = {"Authorization": f"Bearer {common.DAEMON_TOKEN}"}

    mock_root = f"http://127.0.0.1:{args.mock_port}"
    env["NUCLEAR_TELEGRAM_API_BASE_URL"] = f"{mock_root}/telegram"
    env["NUCLEAR_DISCORD_API_BASE_URL"] = f"{mock_root}/discord"
    env["NUCLEAR_SLACK_API_BASE_URL"] = f"{mock_root}/slack"
    env["NUCLEAR_GMAIL_API_BASE_URL"] = f"{mock_root}/gmail"
    env["NUCLEAR_BRAVE_API_BASE_URL"] = f"{mock_root}/brave"

    fixtures_root = scenario_root / "fixtures"
    fixtures_root.mkdir(parents=True, exist_ok=True)
    signal_cli_path = write_signal_cli_fixtures(fixtures_root)
    env["PHASE2_SIGNAL_STATE"] = str(signal_cli_path.parent / "state")

    server = MockServiceServer("127.0.0.1", args.mock_port)
    common.log_step(f"matrix: starting mock service server on 127.0.0.1:{args.mock_port}")
    server.start()

    try:
        common.wait_for_http_json(f"{mock_root}/openai/v1/models")

        common.log_step("matrix: bootstrapping isolated profile")
        doctor = common.run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        doctor_values = common.parse_key_value_output(doctor.stdout)
        config_path = Path(doctor_values["config_path"])
        common.update_base_config(config_path, repo_root, args.daemon_port)

        openai_base_url = f"{mock_root}/openai/v1"
        anthropic_base_url = f"{mock_root}/anthropic"
        ollama_base_url = f"{mock_root}/ollama"
        codex_base_url = f"{mock_root}/codex"
        home_assistant_base_url = f"{mock_root}/home-assistant"

        common.log_step("matrix: configuring provider matrix")
        provider_args = [
            (
                OPENAI_PROVIDER_ID,
                "OpenAI",
                "openai",
                OPENAI_MODEL,
                OPENAI_TOKEN,
                OPENAI_MAIN_ALIAS,
            ),
            (
                MOONSHOT_PROVIDER_ID,
                "Moonshot",
                "moonshot",
                OPENAI_MODEL,
                MOONSHOT_TOKEN,
                MOONSHOT_ALIAS,
            ),
            (
                OPENROUTER_PROVIDER_ID,
                "OpenRouter",
                "openrouter",
                OPENAI_MODEL,
                OPENROUTER_TOKEN,
                OPENROUTER_ALIAS,
            ),
            (
                VENICE_PROVIDER_ID,
                "Venice",
                "venice",
                OPENAI_MODEL,
                VENICE_TOKEN,
                VENICE_ALIAS,
            ),
            (
                ANTHROPIC_PROVIDER_ID,
                "Anthropic",
                "anthropic",
                ANTHROPIC_MODEL,
                ANTHROPIC_TOKEN,
                ANTHROPIC_ALIAS,
            ),
        ]
        for provider_id, name, kind, model, api_key, alias in provider_args:
            common.run_command(
                [
                    str(binary_path),
                    "provider",
                    "add",
                    "--id",
                    provider_id,
                    "--name",
                    name,
                    "--kind",
                    kind,
                    "--base-url",
                    anthropic_base_url if kind == "anthropic" else openai_base_url,
                    "--model",
                    model,
                    "--api-key",
                    api_key,
                    "--main-alias",
                    alias,
                ],
                env=env,
                cwd=repo_root,
            )
        common.run_command(
            [
                str(binary_path),
                "provider",
                "add-local",
                "--id",
                OLLAMA_PROVIDER_ID,
                "--name",
                "Ollama",
                "--kind",
                "ollama",
                "--base-url",
                ollama_base_url,
                "--model",
                OLLAMA_MODEL,
            ],
            env=env,
            cwd=repo_root,
        )
        add_alias(binary_path, env, repo_root, OLLAMA_ALIAS, OLLAMA_PROVIDER_ID, OLLAMA_MODEL)
        common.set_onboarding_complete(config_path, value=True)

        common.log_step("matrix: starting daemon")
        common.run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        common.wait_for_http_json(status_url, headers=auth_headers)

        common.log_step("matrix: finishing runtime provider setup and trust config")
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/providers",
            body=build_codex_provider_request(CODEX_PROVIDER_ID, codex_base_url, CODEX_TOKEN),
        )
        add_alias(binary_path, env, repo_root, CODEX_ALIAS, CODEX_PROVIDER_ID, CODEX_MODEL)

        permissions = common.run_command(
            [str(binary_path), "permissions", "auto-edit"],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(permissions.stdout, "permission_preset=auto-edit", context="permissions output")
        trust = common.run_command(
            [
                str(binary_path),
                "trust",
                "--path",
                str(repo_root),
                "--allow-shell",
                "true",
                "--allow-network",
                "true",
                "--allow-full-disk",
                "false",
                "--allow-self-edit",
                "false",
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(trust.stdout, "network=true", context="trust output")

        common.log_step("matrix: verifying provider health, listing, and prompt execution")
        provider_list = common.run_command([str(binary_path), "provider", "list"], env=env, cwd=repo_root)
        for provider_id in [
            OPENAI_PROVIDER_ID,
            MOONSHOT_PROVIDER_ID,
            OPENROUTER_PROVIDER_ID,
            VENICE_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            OLLAMA_PROVIDER_ID,
            CODEX_PROVIDER_ID,
        ]:
            common.assert_in(provider_list.stdout, provider_id, context="provider list")

        doctor_report = daemon_json(base_url, auth_headers, "GET", "/v1/doctor")
        provider_health = {entry["id"]: entry for entry in doctor_report["providers"]}
        for provider_id in [
            OPENAI_PROVIDER_ID,
            MOONSHOT_PROVIDER_ID,
            OPENROUTER_PROVIDER_ID,
            VENICE_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            OLLAMA_PROVIDER_ID,
            CODEX_PROVIDER_ID,
        ]:
            common.assert_true(
                provider_health[provider_id]["ok"] is True,
                message=f"provider should be healthy: {provider_health[provider_id]}",
            )

        provider_models = {
            OPENAI_PROVIDER_ID: OPENAI_MODEL,
            MOONSHOT_PROVIDER_ID: OPENAI_MODEL,
            OPENROUTER_PROVIDER_ID: OPENAI_MODEL,
            VENICE_PROVIDER_ID: OPENAI_MODEL,
            ANTHROPIC_PROVIDER_ID: ANTHROPIC_MODEL,
            OLLAMA_PROVIDER_ID: OLLAMA_MODEL,
            CODEX_PROVIDER_ID: CODEX_MODEL,
        }
        for provider_id, model in provider_models.items():
            model_list = common.run_command(
                [str(binary_path), "model", "list", "--provider", provider_id],
                env=env,
                cwd=repo_root,
            )
            common.assert_in(model_list.stdout, model, context=f"model list {provider_id}")

        exec_aliases = {
            OPENAI_MAIN_ALIAS: OPENAI_PROVIDER_ID,
            MOONSHOT_ALIAS: MOONSHOT_PROVIDER_ID,
            OPENROUTER_ALIAS: OPENROUTER_PROVIDER_ID,
            VENICE_ALIAS: VENICE_PROVIDER_ID,
            ANTHROPIC_ALIAS: ANTHROPIC_PROVIDER_ID,
            OLLAMA_ALIAS: OLLAMA_PROVIDER_ID,
            CODEX_ALIAS: CODEX_PROVIDER_ID,
        }
        for alias, provider_id in exec_aliases.items():
            prompt = f"Phase 2 provider matrix {alias}"
            event = last_exec_event(binary_path, env, repo_root, alias, prompt)
            common.assert_true(event["event"] == "response", message=f"unexpected exec event: {event}")
            common.assert_true(event["alias"] == alias, message=f"exec alias mismatch: {event}")
            common.assert_true(event["provider_id"] == provider_id, message=f"exec provider mismatch: {event}")
            common.assert_true(prompt in event["response"], message=f"provider response missing prompt: {event}")

        common.log_step("matrix: verifying provider failure messaging")
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/providers",
            body={
                "provider": {
                    "id": "openai-bad-auth",
                    "display_name": "openai-bad-auth",
                    "kind": "open_ai_compatible",
                    "base_url": openai_base_url,
                    "auth_mode": "api_key",
                    "default_model": OPENAI_MODEL,
                    "keychain_account": None,
                    "oauth": None,
                    "local": False,
                },
                "api_key": "bad-openai-key",
                "oauth_token": None,
            },
        )
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/providers",
            body={
                "provider": {
                    "id": "anthropic-bad-auth",
                    "display_name": "anthropic-bad-auth",
                    "kind": "anthropic",
                    "base_url": anthropic_base_url,
                    "auth_mode": "api_key",
                    "default_model": ANTHROPIC_MODEL,
                    "keychain_account": None,
                    "oauth": None,
                    "local": False,
                },
                "api_key": "bad-anthropic-key",
                "oauth_token": None,
            },
        )
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/providers",
            body=build_codex_provider_request("codex-bad-auth", codex_base_url, "bad-codex-token"),
        )
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/providers",
            body=build_ollama_provider_request("ollama-bad-model", ollama_base_url, "missing-ollama"),
        )
        doctor_report = daemon_json(base_url, auth_headers, "GET", "/v1/doctor")
        bad_health = {entry["id"]: entry for entry in doctor_report["providers"]}
        common.assert_in(bad_health["openai-bad-auth"]["detail"], "invalid API key", context="bad openai auth detail")
        common.assert_in(bad_health["anthropic-bad-auth"]["detail"], "invalid x-api-key", context="bad anthropic auth detail")
        common.assert_in(bad_health["codex-bad-auth"]["detail"], "authentication token invalid", context="bad codex auth detail")
        common.assert_in(bad_health["ollama-bad-model"]["detail"], "missing-ollama", context="bad ollama model detail")
        for provider_id in ["openai-bad-auth", "anthropic-bad-auth", "codex-bad-auth", "ollama-bad-model"]:
            daemon_json(base_url, auth_headers, "DELETE", f"/v1/providers/{provider_id}")

        common.log_step("matrix: verifying delegation config and targets")
        targets = daemon_json(base_url, auth_headers, "GET", "/v1/delegation/targets")
        provider_ids = {target["provider_id"] for target in targets}
        for provider_id in [
            OPENAI_PROVIDER_ID,
            MOONSHOT_PROVIDER_ID,
            OPENROUTER_PROVIDER_ID,
            VENICE_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            OLLAMA_PROVIDER_ID,
            CODEX_PROVIDER_ID,
        ]:
            common.assert_true(provider_id in provider_ids, message=f"missing delegation target for {provider_id}")
        updated_delegation = daemon_json(
            base_url,
            auth_headers,
            "PUT",
            "/v1/delegation/config",
            body={
                "max_depth": {"mode": "limited", "value": 2},
                "max_parallel_subagents": {"mode": "limited", "value": 4},
                "disabled_provider_ids": [MOONSHOT_PROVIDER_ID],
            },
        )
        common.assert_true(updated_delegation["max_depth"]["value"] == 2, message=f"unexpected delegation config: {updated_delegation}")
        common.assert_true(updated_delegation["max_parallel_subagents"]["value"] == 4, message=f"unexpected delegation config: {updated_delegation}")
        common.assert_true(updated_delegation["disabled_provider_ids"] == [MOONSHOT_PROVIDER_ID], message=f"unexpected delegation config: {updated_delegation}")
        targets_after_disable = daemon_json(base_url, auth_headers, "GET", "/v1/delegation/targets")
        common.assert_true(
            all(target["provider_id"] != MOONSHOT_PROVIDER_ID for target in targets_after_disable),
            message=f"moonshot should be excluded from delegation targets: {targets_after_disable}",
        )
        daemon_json(
            base_url,
            auth_headers,
            "PUT",
            "/v1/delegation/config",
            body={
                "max_depth": {"mode": "limited", "value": 2},
                "max_parallel_subagents": {"mode": "limited", "value": 4},
                "disabled_provider_ids": [],
            },
        )

        common.log_step("matrix: configuring connectors")
        webhook_add = common.run_command(
            [
                str(binary_path),
                "webhook",
                "add",
                "--id",
                WEBHOOK_ID,
                "--name",
                "Phase 2 Webhook",
                "--description",
                "Phase 2 webhook connector",
                "--prompt-template",
                "Connector: {connector_name}\nSummary: {summary}\nPrompt: {prompt}\nDetails: {details}\nPayload:\n{payload_json}",
                "--token",
                WEBHOOK_TOKEN,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(webhook_add.stdout, WEBHOOK_ID, context="webhook add output")

        common.run_command(
            [
                str(binary_path),
                "telegram",
                "add",
                "--id",
                TELEGRAM_ID,
                "--name",
                "Phase 2 Telegram",
                "--description",
                "Phase 2 telegram connector",
                "--bot-token",
                TELEGRAM_TOKEN,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )
        common.run_command(
            [
                str(binary_path),
                "discord",
                "add",
                "--id",
                DISCORD_ID,
                "--name",
                "Phase 2 Discord",
                "--description",
                "Phase 2 discord connector",
                "--bot-token",
                DISCORD_TOKEN,
                "--monitored-channel-id",
                DISCORD_CHANNEL_ID,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )
        common.run_command(
            [
                str(binary_path),
                "slack",
                "add",
                "--id",
                SLACK_ID,
                "--name",
                "Phase 2 Slack",
                "--description",
                "Phase 2 slack connector",
                "--bot-token",
                SLACK_TOKEN,
                "--monitored-channel-id",
                SLACK_CHANNEL_ID,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )
        common.run_command(
            [
                str(binary_path),
                "signal",
                "add",
                "--id",
                SIGNAL_ID,
                "--name",
                "Phase 2 Signal",
                "--description",
                "Phase 2 signal connector",
                "--account",
                SIGNAL_ACCOUNT,
                "--cli-path",
                str(signal_cli_path),
                "--monitored-group-id",
                SIGNAL_GROUP_ID,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )
        common.run_command(
            [
                str(binary_path),
                "home-assistant",
                "add",
                "--id",
                HOME_ASSISTANT_ID,
                "--name",
                "Phase 2 Home Assistant",
                "--description",
                "Phase 2 Home Assistant connector",
                "--base-url",
                home_assistant_base_url,
                "--access-token",
                HOME_ASSISTANT_TOKEN,
                "--entity-id",
                HOME_ASSISTANT_ENTITY_ID,
                "--service-domain",
                "light",
                "--service-entity-id",
                HOME_ASSISTANT_ENTITY_ID,
                "--alias",
                OPENAI_MAIN_ALIAS,
            ],
            env=env,
            cwd=repo_root,
        )

        gmail_connector = {
            "id": GMAIL_ID,
            "name": "Phase 2 Gmail",
            "description": "Phase 2 gmail connector",
            "enabled": True,
            "oauth_keychain_account": None,
            "require_pairing_approval": True,
            "allowed_sender_addresses": [],
            "label_filter": "INBOX",
            "last_history_id": None,
            "alias": OPENAI_MAIN_ALIAS,
            "requested_model": None,
            "cwd": None,
        }
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/gmail",
            body={"connector": gmail_connector, "oauth_token": GMAIL_TOKEN},
        )
        brave_connector = {
            "id": BRAVE_ID,
            "name": "Phase 2 Brave",
            "description": "Phase 2 brave connector",
            "enabled": True,
            "api_key_keychain_account": None,
            "alias": OPENAI_MAIN_ALIAS,
            "requested_model": None,
            "cwd": None,
        }
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/brave",
            body={"connector": brave_connector, "api_key": BRAVE_TOKEN},
        )

        bootstrap = daemon_json(base_url, auth_headers, "GET", "/v1/dashboard/bootstrap")
        for field in [
            "webhook_connectors",
            "telegram_connectors",
            "discord_connectors",
            "slack_connectors",
            "signal_connectors",
            "home_assistant_connectors",
            "gmail_connectors",
            "brave_connectors",
        ]:
            common.assert_true(
                len(bootstrap[field]) == 1,
                message=f"expected exactly one connector in {field}: {bootstrap[field]}",
            )
        common.assert_true(
            bootstrap["status"]["pending_connector_approvals"] == 0,
            message=f"unexpected pending approvals before polling: {bootstrap['status']}",
        )

        common.log_step("matrix: verifying webhook delivery failure and recovery")
        webhook_failure = common.run_command(
            [str(binary_path), "webhook", "deliver", WEBHOOK_ID, "--summary", "Phase 2 missing token"],
            env=env,
            cwd=repo_root,
            check=False,
        )
        common.assert_true(
            webhook_failure.returncode != 0,
            message="webhook delivery without a token should fail",
        )
        common.assert_in(
            command_failure_output(webhook_failure),
            "missing webhook token",
            context="webhook missing token output",
        )
        webhook_success = common.run_command(
            [
                str(binary_path),
                "webhook",
                "deliver",
                WEBHOOK_ID,
                "--summary",
                "Phase 2 webhook summary",
                "--prompt",
                "Phase 2 webhook prompt",
                "--details",
                "Phase 2 webhook details",
                "--token",
                WEBHOOK_TOKEN,
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            webhook_success.stdout,
            "queued webhook mission=",
            context="webhook delivery output",
        )

        common.log_step("matrix: verifying telegram approval, allowlist, and send flow")
        telegram_poll = common.run_command(
            [str(binary_path), "telegram", "poll", TELEGRAM_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(telegram_poll.stdout, f"telegram='{TELEGRAM_ID}'", context="telegram poll output")
        telegram_approvals = wait_for_cli_json(
            binary_path,
            env,
            repo_root,
            "telegram",
            "approvals",
            "list",
            "--json",
            predicate=lambda payload: len(payload) == 1,
            description="telegram pending approval",
        )
        telegram_approval_id = telegram_approvals[0]["id"]
        telegram_approve = common.run_command(
            [str(binary_path), "telegram", "approvals", "approve", telegram_approval_id],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            telegram_approve.stdout,
            "approved telegram pairing=",
            context="telegram approval output",
        )
        telegram_get = common.run_command(
            [str(binary_path), "telegram", "get", TELEGRAM_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(telegram_get.stdout, f"chat_ids={TELEGRAM_CHAT_ID}", context="telegram get output")
        common.assert_in(telegram_get.stdout, f"user_ids={TELEGRAM_USER_ID}", context="telegram get output")
        telegram_send = common.run_command(
            [
                str(binary_path),
                "telegram",
                "send",
                "--id",
                TELEGRAM_ID,
                "--chat-id",
                str(TELEGRAM_CHAT_ID),
                "--text",
                "Phase 2 telegram response",
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            telegram_send.stdout,
            "message_id=",
            context="telegram send output",
        )

        common.log_step("matrix: verifying discord approval, allowlist, and send flow")
        discord_poll = common.run_command(
            [str(binary_path), "discord", "poll", DISCORD_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(discord_poll.stdout, f"discord='{DISCORD_ID}'", context="discord poll output")
        discord_approvals = wait_for_cli_json(
            binary_path,
            env,
            repo_root,
            "discord",
            "approvals",
            "list",
            "--json",
            predicate=lambda payload: len(payload) == 1,
            description="discord pending approval",
        )
        discord_approval_id = discord_approvals[0]["id"]
        discord_approve = common.run_command(
            [str(binary_path), "discord", "approvals", "approve", discord_approval_id],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            discord_approve.stdout,
            "approved discord pairing=",
            context="discord approval output",
        )
        discord_get = common.run_command(
            [str(binary_path), "discord", "get", DISCORD_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            discord_get.stdout,
            f"allowed_channel_ids={DISCORD_CHANNEL_ID}",
            context="discord get output",
        )
        common.assert_in(
            discord_get.stdout,
            f"allowed_user_ids={DISCORD_USER_ID}",
            context="discord get output",
        )
        discord_send = common.run_command(
            [
                str(binary_path),
                "discord",
                "send",
                "--id",
                DISCORD_ID,
                "--channel-id",
                DISCORD_CHANNEL_ID,
                "--content",
                "Phase 2 discord response",
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            discord_send.stdout,
            "message_id=",
            context="discord send output",
        )

        common.log_step("matrix: verifying slack approval, allowlist, and send flow")
        slack_poll = common.run_command(
            [str(binary_path), "slack", "poll", SLACK_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(slack_poll.stdout, f"slack='{SLACK_ID}'", context="slack poll output")
        slack_approvals = wait_for_cli_json(
            binary_path,
            env,
            repo_root,
            "slack",
            "approvals",
            "list",
            "--json",
            predicate=lambda payload: len(payload) == 1,
            description="slack pending approval",
        )
        slack_approval_id = slack_approvals[0]["id"]
        slack_approve = common.run_command(
            [str(binary_path), "slack", "approvals", "approve", slack_approval_id],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            slack_approve.stdout,
            "approved slack pairing=",
            context="slack approval output",
        )
        slack_get = common.run_command(
            [str(binary_path), "slack", "get", SLACK_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            slack_get.stdout,
            f"allowed_channel_ids={SLACK_CHANNEL_ID}",
            context="slack get output",
        )
        common.assert_in(
            slack_get.stdout,
            f"allowed_user_ids={SLACK_USER_ID}",
            context="slack get output",
        )
        slack_send = common.run_command(
            [
                str(binary_path),
                "slack",
                "send",
                "--id",
                SLACK_ID,
                "--channel-id",
                SLACK_CHANNEL_ID,
                "--text",
                "Phase 2 slack response",
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(slack_send.stdout, "ts=", context="slack send output")

        common.log_step("matrix: verifying signal approval, allowlist, and send flow")
        signal_poll = common.run_command(
            [str(binary_path), "signal", "poll", SIGNAL_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(signal_poll.stdout, f"signal='{SIGNAL_ID}'", context="signal poll output")
        signal_approvals = wait_for_cli_json(
            binary_path,
            env,
            repo_root,
            "signal",
            "approvals",
            "list",
            "--json",
            predicate=lambda payload: len(payload) == 1,
            description="signal pending approval",
        )
        signal_approval_id = signal_approvals[0]["id"]
        signal_approve = common.run_command(
            [str(binary_path), "signal", "approvals", "approve", signal_approval_id],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            signal_approve.stdout,
            "approved signal pairing=",
            context="signal approval output",
        )
        signal_get = common.run_command(
            [str(binary_path), "signal", "get", SIGNAL_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            signal_get.stdout,
            f"allowed_group_ids={SIGNAL_GROUP_ID}",
            context="signal get output",
        )
        common.assert_in(
            signal_get.stdout,
            f"allowed_user_ids={SIGNAL_USER_ID}",
            context="signal get output",
        )
        signal_send = common.run_command(
            [
                str(binary_path),
                "signal",
                "send",
                "--id",
                SIGNAL_ID,
                "--group-id",
                SIGNAL_GROUP_ID,
                "--text",
                "Phase 2 signal response",
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            signal_send.stdout,
            f"group {SIGNAL_GROUP_ID}",
            context="signal send output",
        )

        common.log_step("matrix: verifying home assistant poll, restriction, and service flow")
        home_state_before = common.run_command(
            [
                str(binary_path),
                "home-assistant",
                "state",
                HOME_ASSISTANT_ID,
                "--entity-id",
                HOME_ASSISTANT_ENTITY_ID,
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(home_state_before.stdout, '"state": "off"', context="home assistant state output")
        home_poll_initial = common.run_command(
            [str(binary_path), "home-assistant", "poll", HOME_ASSISTANT_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            home_poll_initial.stdout,
            "queued_missions=0",
            context="home assistant initial poll output",
        )
        server.state.set_home_entity_state(
            HOME_ASSISTANT_ENTITY_ID,
            "on",
            "2026-03-28T12:05:00Z",
        )
        home_poll_changed = common.run_command(
            [str(binary_path), "home-assistant", "poll", HOME_ASSISTANT_ID],
            env=env,
            cwd=repo_root,
        )
        wait_for_cli_output_contains(
            binary_path,
            env,
            repo_root,
            ["mission", "list"],
            "Home Assistant: Office Light changed to on",
            description="home assistant state-change mission",
        )
        forbidden_service = common.run_command(
            [
                str(binary_path),
                "home-assistant",
                "call-service",
                "--id",
                HOME_ASSISTANT_ID,
                "--domain",
                "lock",
                "--service",
                "unlock",
                "--entity-id",
                HOME_ASSISTANT_ENTITY_ID,
            ],
            env=env,
            cwd=repo_root,
            check=False,
        )
        common.assert_true(
            forbidden_service.returncode != 0,
            message="forbidden Home Assistant domain should fail",
        )
        common.assert_in(
            command_failure_output(forbidden_service),
            "not allowed",
            context="home assistant forbidden service output",
        )
        allowed_service = common.run_command(
            [
                str(binary_path),
                "home-assistant",
                "call-service",
                "--id",
                HOME_ASSISTANT_ID,
                "--domain",
                "light",
                "--service",
                "turn_on",
                "--entity-id",
                HOME_ASSISTANT_ENTITY_ID,
            ],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            allowed_service.stdout,
            "changed_entities=1",
            context="home assistant allowed service output",
        )

        common.log_step("matrix: verifying gmail approval, allowlist, and send flow")
        gmail_poll = daemon_json(
            base_url,
            auth_headers,
            "POST",
            f"/v1/gmail/{GMAIL_ID}/poll",
            body={},
        )
        common.assert_true(
            gmail_poll["connector_id"] == GMAIL_ID,
            message=f"unexpected gmail poll response: {gmail_poll}",
        )
        gmail_approvals = wait_for_daemon_json(
            base_url,
            auth_headers,
            "/v1/connector-approvals?kind=gmail&status=pending&limit=10",
            predicate=lambda payload: len(payload) == 1,
            description="gmail pending approval",
        )
        gmail_approval_id = gmail_approvals[0]["id"]
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            f"/v1/connector-approvals/{gmail_approval_id}/approve",
            body={"note": "phase2 gmail approval"},
        )
        gmail_after_approve = daemon_json(base_url, auth_headers, "GET", f"/v1/gmail/{GMAIL_ID}")
        common.assert_true(
            GMAIL_SENDER in gmail_after_approve["allowed_sender_addresses"],
            message=f"gmail allowlist did not persist approval: {gmail_after_approve}",
        )
        gmail_send = daemon_json(
            base_url,
            auth_headers,
            "POST",
            f"/v1/gmail/{GMAIL_ID}/send",
            body={
                "to": "status@example.com",
                "subject": "Phase 2 Gmail response",
                "body": "Gmail operator flow succeeded.",
            },
        )
        common.assert_true(
            bool(gmail_send["message_id"]),
            message=f"gmail send did not return a message id: {gmail_send}",
        )

        common.log_step("matrix: verifying brave connector error and recovery flow")
        brave_list = daemon_json(base_url, auth_headers, "GET", "/v1/brave")
        common.assert_true(len(brave_list) == 1, message=f"unexpected brave connectors: {brave_list}")
        daemon_json(
            base_url,
            auth_headers,
            "POST",
            "/v1/brave",
            body={
                "connector": {
                    "id": BRAVE_ALT_ID,
                    "name": "Phase 2 Brave Alt",
                    "description": "Phase 2 brave alternate connector",
                    "enabled": True,
                    "api_key_keychain_account": None,
                    "alias": OPENAI_MAIN_ALIAS,
                    "requested_model": None,
                    "cwd": None,
                },
                "api_key": BRAVE_TOKEN,
            },
        )
        brave_multi_event = last_exec_event(
            binary_path,
            env,
            repo_root,
            OPENAI_MAIN_ALIAS,
            BRAVE_PROMPT,
            permissions="full-auto",
        )
        common.assert_true(
            "multiple brave connectors are enabled" in brave_multi_event["response"],
            message=f"expected brave ambiguity error in exec response: {brave_multi_event}",
        )
        daemon_json(base_url, auth_headers, "DELETE", f"/v1/brave/{BRAVE_ALT_ID}")
        brave_single_event = last_exec_event(
            binary_path,
            env,
            repo_root,
            OPENAI_MAIN_ALIAS,
            BRAVE_PROMPT,
            permissions="full-auto",
        )
        common.assert_true(
            "Phase 2 Brave Result" in brave_single_event["response"],
            message=f"expected brave search result in exec response: {brave_single_event}",
        )
        common.assert_true(
            any("phase 2 brave validation" in query for query in server.state.brave_queries),
            message=f"brave query never reached the mock service: {server.state.brave_queries}",
        )

        common.log_step("matrix: verifying restart persistence for provider and connector surfaces")
        daemon_json(base_url, auth_headers, "POST", "/v1/shutdown")
        common.wait_for_daemon_down(status_url, headers=auth_headers, timeout=30.0)
        common.run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        common.wait_for_http_json(status_url, headers=auth_headers)

        bootstrap_after_restart = daemon_json(base_url, auth_headers, "GET", "/v1/dashboard/bootstrap")
        for field in [
            "webhook_connectors",
            "telegram_connectors",
            "discord_connectors",
            "slack_connectors",
            "signal_connectors",
            "home_assistant_connectors",
            "gmail_connectors",
            "brave_connectors",
        ]:
            common.assert_true(
                len(bootstrap_after_restart[field]) == 1,
                message=f"expected restart persistence for {field}: {bootstrap_after_restart[field]}",
            )
        common.assert_true(
            bootstrap_after_restart["status"]["pending_connector_approvals"] == 0,
            message=f"pending approvals should be clear after restart: {bootstrap_after_restart['status']}",
        )
        telegram_after_restart = common.run_command(
            [str(binary_path), "telegram", "get", TELEGRAM_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            telegram_after_restart.stdout,
            f"chat_ids={TELEGRAM_CHAT_ID}",
            context="telegram restart persistence output",
        )
        discord_after_restart = common.run_command(
            [str(binary_path), "discord", "get", DISCORD_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            discord_after_restart.stdout,
            f"allowed_channel_ids={DISCORD_CHANNEL_ID}",
            context="discord restart persistence output",
        )
        slack_after_restart = common.run_command(
            [str(binary_path), "slack", "get", SLACK_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            slack_after_restart.stdout,
            f"allowed_channel_ids={SLACK_CHANNEL_ID}",
            context="slack restart persistence output",
        )
        signal_after_restart = common.run_command(
            [str(binary_path), "signal", "get", SIGNAL_ID],
            env=env,
            cwd=repo_root,
        )
        common.assert_in(
            signal_after_restart.stdout,
            f"allowed_group_ids={SIGNAL_GROUP_ID}",
            context="signal restart persistence output",
        )
        gmail_after_restart = daemon_json(base_url, auth_headers, "GET", f"/v1/gmail/{GMAIL_ID}")
        common.assert_true(
            GMAIL_SENDER in gmail_after_restart["allowed_sender_addresses"],
            message=f"gmail allowlist missing after restart: {gmail_after_restart}",
        )

        common.log_step("matrix: cleaning up connector matrix state")
        daemon_json(base_url, auth_headers, "DELETE", f"/v1/brave/{BRAVE_ID}")
        daemon_json(base_url, auth_headers, "DELETE", f"/v1/gmail/{GMAIL_ID}")
        common.run_command([str(binary_path), "home-assistant", "remove", HOME_ASSISTANT_ID], env=env, cwd=repo_root)
        common.run_command([str(binary_path), "signal", "remove", SIGNAL_ID], env=env, cwd=repo_root)
        common.run_command([str(binary_path), "slack", "remove", SLACK_ID], env=env, cwd=repo_root)
        common.run_command([str(binary_path), "discord", "remove", DISCORD_ID], env=env, cwd=repo_root)
        common.run_command([str(binary_path), "telegram", "remove", TELEGRAM_ID], env=env, cwd=repo_root)
        common.run_command([str(binary_path), "webhook", "remove", WEBHOOK_ID], env=env, cwd=repo_root)

        bootstrap_after_cleanup = daemon_json(base_url, auth_headers, "GET", "/v1/dashboard/bootstrap")
        for field in [
            "webhook_connectors",
            "telegram_connectors",
            "discord_connectors",
            "slack_connectors",
            "signal_connectors",
            "home_assistant_connectors",
            "gmail_connectors",
            "brave_connectors",
        ]:
            common.assert_true(
                len(bootstrap_after_cleanup[field]) == 0,
                message=f"expected cleanup to remove all connectors from {field}: {bootstrap_after_cleanup[field]}",
            )
        common.assert_true(
            bootstrap_after_cleanup["status"]["pending_connector_approvals"] == 0,
            message=f"cleanup left pending approvals behind: {bootstrap_after_cleanup['status']}",
        )

        print("Phase 2 matrix passed.")
        print(f"config_path={config_path}")
        print(f"data_path={doctor_values['data_path']}")
        return 0
    finally:
        try:
            http_request_json(
                "POST",
                f"{base_url}/v1/shutdown",
                headers=auth_headers,
                body={},
                expected_status=None,
            )
        except Exception:
            pass
        try:
            common.wait_for_daemon_down(status_url, headers=auth_headers, timeout=5.0)
        except Exception:
            pass
        common.run_command(
            [str(binary_path), "daemon", "stop"],
            env=env,
            cwd=repo_root,
            check=False,
            capture_output=False,
        )
        server.stop()


if __name__ == "__main__":
    raise SystemExit(main())
