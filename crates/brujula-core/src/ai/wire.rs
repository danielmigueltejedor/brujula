//! How a request to each provider is written and its answer read. One conversation, in one shape
//! (`Message`), goes out in the protocol of OpenAI, Claude or Gemini. Ollama and any custom
//! server speak the OpenAI one.
//!
//! The key goes in a header, never in the address, so it cannot end up in a log or a history.

use serde_json::{json, Value};

use super::settings::{AiSettings, Provider};

/// How many tokens an answer may take. A browser assistant answers short.
const MAX_OUTPUT_TOKENS: u32 = 2048;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// What the provider calls this call, so its result can be matched. Gemini has none: the
    /// name is used.
    pub id: String,
    pub name: String,
    pub args: Value,
    /// Gemini wants its "thought signature" given back with the call; kept as it came.
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Call(ToolCall),
    /// What a tool answered.
    Result {
        id: String,
        name: String,
        content: String,
        error: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    /// Results of tools. Claude and Gemini carry them as a user turn; OpenAI as `tool` messages.
    Tool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text(text.to_owned())],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON schema of the arguments.
    pub parameters: Value,
}

/// What to send: the address, the headers and the body.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// What came back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<ToolCall>,
}

/// A conversation that has read many pages grows. The oldest results of tools are replaced by a
/// short note until what is left fits in `budget` characters; the questions and answers stay.
pub fn trim_results(history: &mut [Message], budget: usize) {
    const OMITTED: &str = "[contenido anterior omitido para ahorrar espacio]";
    let mut total: usize = history
        .iter()
        .flat_map(|m| &m.parts)
        .map(|p| match p {
            Part::Result { content, .. } => content.chars().count(),
            _ => 0,
        })
        .sum();
    for p in history.iter_mut().flat_map(|m| &mut m.parts) {
        if total <= budget {
            return;
        }
        if let Part::Result { content, .. } = p {
            let n = content.chars().count();
            if n > OMITTED.chars().count() {
                total -= n - OMITTED.chars().count();
                *content = OMITTED.to_owned();
            }
        }
    }
}

/// The request for the next turn of `history`. `key` is empty for a server that needs none.
pub fn build_request(
    settings: &AiSettings,
    key: &str,
    system: &str,
    history: &[Message],
    tools: &[ToolSpec],
) -> Result<Request, String> {
    let base = settings.endpoint()?;
    let base = base.as_str().trim_end_matches('/').to_owned();
    let model = settings.model();
    let mut headers = vec![("Content-Type".to_owned(), "application/json".to_owned())];
    let (url, body) = match settings.provider {
        Provider::Anthropic => {
            headers.push(("x-api-key".to_owned(), key.to_owned()));
            headers.push(("anthropic-version".to_owned(), "2023-06-01".to_owned()));
            (
                format!("{base}/messages"),
                anthropic_body(model, system, history, tools),
            )
        }
        Provider::Gemini => {
            headers.push(("x-goog-api-key".to_owned(), key.to_owned()));
            (
                format!("{base}/models/{model}:generateContent"),
                gemini_body(system, history, tools),
            )
        }
        Provider::OpenAi | Provider::Ollama | Provider::Custom => {
            if !key.is_empty() {
                headers.push(("Authorization".to_owned(), format!("Bearer {key}")));
            }
            (
                format!("{base}/chat/completions"),
                openai_body(model, system, history, tools),
            )
        }
    };
    Ok(Request {
        url,
        headers,
        body: body.to_string(),
    })
}

/// A tool call's arguments as text, the way OpenAI wants them back.
fn args_text(args: &Value) -> String {
    args.to_string()
}

fn openai_body(model: &str, system: &str, history: &[Message], tools: &[ToolSpec]) -> Value {
    let mut messages = vec![json!({"role": "system", "content": system})];
    for m in history {
        match m.role {
            Role::User => {
                let text: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(json!({"role": "user", "content": text}));
            }
            Role::Assistant => {
                let text: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let calls: Vec<Value> = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Call(c) => Some(json!({
                            "id": c.id,
                            "type": "function",
                            "function": {"name": c.name, "arguments": args_text(&c.args)},
                        })),
                        _ => None,
                    })
                    .collect();
                let mut msg = json!({"role": "assistant", "content": text});
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                messages.push(msg);
            }
            Role::Tool => {
                for p in &m.parts {
                    if let Part::Result { id, content, .. } = p {
                        messages
                            .push(json!({"role": "tool", "tool_call_id": id, "content": content}));
                    }
                }
            }
        }
    }
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": MAX_OUTPUT_TOKENS,
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| {
                json!({"type": "function", "function": {
                    "name": t.name, "description": t.description, "parameters": t.parameters,
                }})
            })
            .collect();
    }
    body
}

fn anthropic_body(model: &str, system: &str, history: &[Message], tools: &[ToolSpec]) -> Value {
    let messages: Vec<Value> = history
        .iter()
        .map(|m| {
            let content: Vec<Value> = m
                .parts
                .iter()
                .map(|p| match p {
                    Part::Text(t) => json!({"type": "text", "text": t}),
                    Part::Call(c) => {
                        json!({"type": "tool_use", "id": c.id, "name": c.name, "input": c.args})
                    }
                    Part::Result {
                        id,
                        content,
                        error,
                        ..
                    } => json!({
                        "type": "tool_result", "tool_use_id": id, "content": content, "is_error": error,
                    }),
                })
                .collect();
            // Results are a user turn for Claude.
            let role = if m.role == Role::Assistant {
                "assistant"
            } else {
                "user"
            };
            json!({"role": role, "content": content})
        })
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_OUTPUT_TOKENS,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
            .collect();
    }
    body
}

fn gemini_body(system: &str, history: &[Message], tools: &[ToolSpec]) -> Value {
    let contents: Vec<Value> = history
        .iter()
        .map(|m| {
            let parts: Vec<Value> = m
                .parts
                .iter()
                .map(|p| match p {
                    Part::Text(t) => json!({"text": t}),
                    Part::Call(c) => {
                        let mut part = json!({"functionCall": {"name": c.name, "args": c.args}});
                        if let Some(sig) = &c.signature {
                            part["thoughtSignature"] = json!(sig);
                        }
                        part
                    }
                    Part::Result {
                        name,
                        content,
                        error,
                        ..
                    } => {
                        let key = if *error { "error" } else { "output" };
                        json!({"functionResponse": {"name": name, "response": {key: content}}})
                    }
                })
                .collect();
            let role = if m.role == Role::Assistant {
                "model"
            } else {
                "user"
            };
            json!({"role": role, "parts": parts})
        })
        .collect();
    let mut body = json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": contents,
        "generationConfig": {"maxOutputTokens": MAX_OUTPUT_TOKENS},
    });
    if !tools.is_empty() {
        body["tools"] = json!([{"functionDeclarations": tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "parameters": t.parameters}))
            .collect::<Vec<_>>()}]);
    }
    body
}

/// Reads the answer of `provider`.
pub fn parse_reply(provider: Provider, body: &str) -> Result<Reply, String> {
    let v: Value = serde_json::from_str(body).map_err(|_| "Respuesta no válida".to_owned())?;
    match provider {
        Provider::Anthropic => parse_anthropic(&v),
        Provider::Gemini => parse_gemini(&v),
        _ => parse_openai(&v),
    }
}

fn parse_openai(v: &Value) -> Result<Reply, String> {
    let msg = v["choices"][0]["message"]
        .as_object()
        .ok_or("La respuesta no trae mensaje")?;
    let mut reply = Reply {
        text: msg
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        calls: Vec::new(),
    };
    for (i, c) in msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let name = c["function"]["name"].as_str().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        // Arguments come as text; a model sometimes sends them broken, which is an empty call.
        let args = c["function"]["arguments"]
            .as_str()
            .and_then(|a| serde_json::from_str::<Value>(a).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        reply.calls.push(ToolCall {
            id: c["id"]
                .as_str()
                .map_or_else(|| format!("call_{i}"), str::to_owned),
            name: name.to_owned(),
            args,
            signature: None,
        });
    }
    Ok(reply)
}

fn parse_anthropic(v: &Value) -> Result<Reply, String> {
    let blocks = v["content"]
        .as_array()
        .ok_or("La respuesta no trae contenido")?;
    let mut reply = Reply::default();
    for b in blocks {
        match b["type"].as_str() {
            Some("text") => {
                if !reply.text.is_empty() {
                    reply.text.push('\n');
                }
                reply.text.push_str(b["text"].as_str().unwrap_or_default());
            }
            Some("tool_use") => reply.calls.push(ToolCall {
                id: b["id"].as_str().unwrap_or_default().to_owned(),
                name: b["name"].as_str().unwrap_or_default().to_owned(),
                args: if b["input"].is_object() {
                    b["input"].clone()
                } else {
                    json!({})
                },
                signature: None,
            }),
            _ => {}
        }
    }
    Ok(reply)
}

fn parse_gemini(v: &Value) -> Result<Reply, String> {
    let parts = v["candidates"][0]["content"]["parts"].as_array();
    let Some(parts) = parts else {
        // A blocked answer has no content, only a reason.
        let why = v["promptFeedback"]["blockReason"]
            .as_str()
            .or_else(|| v["candidates"][0]["finishReason"].as_str());
        return Err(match why {
            Some(w) => format!("Gemini no respondió ({w})"),
            None => "La respuesta no trae contenido".to_owned(),
        });
    };
    let mut reply = Reply::default();
    for (i, p) in parts.iter().enumerate() {
        // Thinking text is the model's own notes, not an answer.
        if p["thought"].as_bool() == Some(true) {
            continue;
        }
        if let Some(t) = p["text"].as_str() {
            reply.text.push_str(t);
        } else if let Some(f) = p.get("functionCall") {
            reply.calls.push(ToolCall {
                id: format!("{}#{i}", f["name"].as_str().unwrap_or_default()),
                name: f["name"].as_str().unwrap_or_default().to_owned(),
                args: if f["args"].is_object() {
                    f["args"].clone()
                } else {
                    json!({})
                },
                signature: p["thoughtSignature"].as_str().map(str::to_owned),
            });
        }
    }
    Ok(reply)
}

/// What the user is told when the provider refuses. A refused key gets no detail at all: the
/// provider's text may quote part of it.
pub fn describe_error(status: u32, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .or_else(|| v["message"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let detail: String = detail
        .chars()
        .filter(|_| !matches!(status, 401 | 403))
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let what = match status {
        400 => "El proveedor no entendió la petición",
        401 | 403 => "El proveedor rechazó la clave (¿es correcta y tiene saldo?)",
        404 => "El modelo o la dirección no existen",
        408 | 504 => "El proveedor tardó demasiado",
        413 => "La conversación es demasiado larga",
        429 => "Demasiadas peticiones o sin saldo; espera un poco",
        500..=599 => "El proveedor tiene un problema ahora mismo",
        _ => "El proveedor devolvió un error",
    };
    if detail.is_empty() {
        format!("{what} (código {status})")
    } else {
        format!("{what} (código {status}): {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "open_tab",
            description: "Opens a tab",
            parameters: json!({"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}),
        }]
    }

    fn convo() -> Vec<Message> {
        vec![
            Message::user("abre example.com"),
            Message {
                role: Role::Assistant,
                parts: vec![
                    Part::Text("Voy.".into()),
                    Part::Call(ToolCall {
                        id: "c1".into(),
                        name: "open_tab".into(),
                        args: json!({"url": "https://example.com"}),
                        signature: Some("sig".into()),
                    }),
                ],
            },
            Message {
                role: Role::Tool,
                parts: vec![Part::Result {
                    id: "c1".into(),
                    name: "open_tab".into(),
                    content: "ok".into(),
                    error: false,
                }],
            },
        ]
    }

    fn settings(p: Provider) -> AiSettings {
        let mut s = AiSettings::default();
        s.apply("provider", p.id()).unwrap();
        s
    }

    #[test]
    fn old_page_text_is_dropped_first_when_the_conversation_grows() {
        let result = |c: &str| Message {
            role: Role::Tool,
            parts: vec![Part::Result {
                id: "x".into(),
                name: "read_page".into(),
                content: c.into(),
                error: false,
            }],
        };
        let mut h = vec![
            Message::user("hola"),
            result(&"a".repeat(1000)),
            result(&"b".repeat(1000)),
        ];
        trim_results(&mut h, 1200);
        let texts: Vec<&str> = h
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|p| match p {
                Part::Result { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts[0].contains("omitido"), "{texts:?}");
        assert_eq!(texts[1].len(), 1000);
        assert_eq!(h[0], Message::user("hola"));
        trim_results(&mut h, 1_000_000);
    }

    #[test]
    fn openai_request_has_the_key_in_a_header_only() {
        let r = build_request(
            &settings(Provider::OpenAi),
            "sk-secret",
            "sys",
            &convo(),
            &tools(),
        )
        .unwrap();
        assert_eq!(r.url, "https://api.openai.com/v1/chat/completions");
        assert!(r
            .headers
            .contains(&("Authorization".into(), "Bearer sk-secret".into())));
        assert!(!r.url.contains("sk-secret"));
        assert!(!r.body.contains("sk-secret"));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(
            b["messages"][2]["tool_calls"][0]["function"]["name"],
            "open_tab"
        );
        // Arguments go back as text, as the protocol wants.
        assert!(b["messages"][2]["tool_calls"][0]["function"]["arguments"].is_string());
        assert_eq!(b["messages"][3]["role"], "tool");
        assert_eq!(b["messages"][3]["tool_call_id"], "c1");
        assert_eq!(b["tools"][0]["function"]["name"], "open_tab");
    }

    #[test]
    fn ollama_sends_no_authorization_and_stays_on_this_computer() {
        let r = build_request(&settings(Provider::Ollama), "", "s", &convo(), &[]).unwrap();
        assert_eq!(r.url, "http://localhost:11434/v1/chat/completions");
        assert!(r.headers.iter().all(|(k, _)| k != "Authorization"));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert!(b.get("tools").is_none());
    }

    #[test]
    fn a_remote_http_server_is_refused_before_any_request_is_built() {
        let mut s = settings(Provider::Custom);
        s.base_url = "http://192.168.1.5:8000/v1".into();
        assert!(build_request(&s, "k", "s", &[], &[]).is_err());
    }

    #[test]
    fn anthropic_request_matches_the_messages_api() {
        let r = build_request(
            &settings(Provider::Anthropic),
            "k",
            "sys",
            &convo(),
            &tools(),
        )
        .unwrap();
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.contains(&("x-api-key".into(), "k".into())));
        assert!(r
            .headers
            .contains(&("anthropic-version".into(), "2023-06-01".into())));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["system"], "sys");
        assert_eq!(b["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(b["messages"][2]["role"], "user");
        assert_eq!(b["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(b["messages"][2]["content"][0]["tool_use_id"], "c1");
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn gemini_request_keeps_the_signature_and_puts_the_key_in_a_header() {
        let r = build_request(&settings(Provider::Gemini), "k", "sys", &convo(), &tools()).unwrap();
        assert_eq!(
            r.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert!(r.headers.contains(&("x-goog-api-key".into(), "k".into())));
        assert!(!r.url.contains("key="));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][1]["parts"][1]["thoughtSignature"], "sig");
        assert_eq!(
            b["contents"][2]["parts"][0]["functionResponse"]["name"],
            "open_tab"
        );
        assert_eq!(b["tools"][0]["functionDeclarations"][0]["name"], "open_tab");
    }

    #[test]
    fn openai_answers_are_read_with_text_and_calls() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hola","tool_calls":[{"id":"abc","type":"function","function":{"name":"open_tab","arguments":"{\"url\":\"https://a.example\"}"}},{"id":"bad","type":"function","function":{"name":"list_tabs","arguments":"{not json"}}]}}]}"#;
        let r = parse_reply(Provider::OpenAi, body).unwrap();
        assert_eq!(r.text, "Hola");
        assert_eq!(r.calls.len(), 2);
        assert_eq!(r.calls[0].args["url"], "https://a.example");
        assert_eq!(r.calls[1].args, json!({}));
        assert!(parse_reply(Provider::OpenAi, "{}").is_err());
        assert!(parse_reply(Provider::OpenAi, "no").is_err());
    }

    #[test]
    fn anthropic_answers_are_read() {
        let body = r#"{"content":[{"type":"text","text":"Voy"},{"type":"tool_use","id":"t1","name":"list_tabs","input":{}}],"stop_reason":"tool_use"}"#;
        let r = parse_reply(Provider::Anthropic, body).unwrap();
        assert_eq!(r.text, "Voy");
        assert_eq!(r.calls[0].id, "t1");
        assert_eq!(r.calls[0].name, "list_tabs");
    }

    #[test]
    fn gemini_answers_skip_thoughts_and_report_blocks() {
        let body = r#"{"candidates":[{"content":{"parts":[{"text":"pienso","thought":true},{"text":"Listo"},{"functionCall":{"name":"read_page","args":{"tab_id":2}},"thoughtSignature":"s1"}]}}]}"#;
        let r = parse_reply(Provider::Gemini, body).unwrap();
        assert_eq!(r.text, "Listo");
        assert_eq!(r.calls[0].args["tab_id"], 2);
        assert_eq!(r.calls[0].signature.as_deref(), Some("s1"));
        let blocked = parse_reply(
            Provider::Gemini,
            r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#,
        );
        assert!(blocked.unwrap_err().contains("SAFETY"));
    }

    #[test]
    fn errors_are_short_and_never_hold_control_characters() {
        let msg = describe_error(401, r#"{"error":{"message":"Incorrect API key\nsk-abc"}}"#);
        assert!(msg.contains("clave") && msg.contains("401"));
        assert!(!msg.contains('\n'));
        assert!(describe_error(500, "<html>").contains("500"));
        assert!(describe_error(429, "").contains("429"));
    }
}
