//! What the user chose for the assistant. Saved per profile in `ai.json`. Everything is off or
//! at its most cautious until the user changes it.

use serde::{Deserialize, Serialize};
use url::Url;

const MAX_MODEL: usize = 80;
const MAX_BLOCKED: usize = 100;

/// Who answers. The first three are cloud services that need the user's own key; Ollama runs on
/// this computer; «Custom» is any server that speaks the OpenAI protocol (LM Studio, llama.cpp,
/// OpenRouter, a company gateway…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
    Custom,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::OpenAi,
        Provider::Anthropic,
        Provider::Gemini,
        Provider::Ollama,
        Provider::Custom,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
            Provider::Ollama => "ollama",
            Provider::Custom => "custom",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The name the user knows it by.
    pub fn label(self) -> &'static str {
        match self {
            Provider::OpenAi => "ChatGPT",
            Provider::Anthropic => "Claude",
            Provider::Gemini => "Gemini",
            Provider::Ollama => "Ollama (local)",
            Provider::Custom => "Otro servidor",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Claude",
            Provider::Gemini => "Gemini",
            Provider::Ollama => "Ollama",
            Provider::Custom => "tu servidor",
        }
    }

    /// The model used when the user has not typed one. They can always type another.
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::OpenAi => "gpt-4.1-mini",
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::Gemini => "gemini-2.5-flash",
            Provider::Ollama => "llama3.1",
            Provider::Custom => "",
        }
    }

    /// Where it lives. The cloud services have a fixed address: the user's key is only ever sent
    /// there. Ollama and the custom server take the address the user typed.
    pub fn default_base(self) -> &'static str {
        match self {
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            Provider::Ollama => "http://localhost:11434/v1",
            Provider::Custom => "",
        }
    }

    pub fn takes_address(self) -> bool {
        matches!(self, Provider::Ollama | Provider::Custom)
    }

    /// The services that do not answer without a key.
    pub fn needs_key(self) -> bool {
        matches!(
            self,
            Provider::OpenAi | Provider::Anthropic | Provider::Gemini
        )
    }

    /// Whether a key can be kept for it at all (Ollama never uses one).
    pub fn can_have_key(self) -> bool {
        self != Provider::Ollama
    }
}

/// How the assistant opens tabs: asking each time, or on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Autonomy {
    #[default]
    Ask,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// The master switch. Off: no button, no panel, no request, no key read.
    pub enabled: bool,
    pub provider: Provider,
    /// The model name; empty means the provider's default.
    pub model: String,
    /// Address of Ollama or of the custom server; ignored for the cloud services.
    pub base_url: String,
    /// The search box hands free text to the assistant (a question is answered, a task is done).
    pub search: bool,
    /// The assistant may see the list of tabs and read pages.
    pub read: bool,
    /// Ask before reading a site for the first time in a conversation.
    pub confirm_reads: bool,
    /// Whether opening tabs needs a yes each time.
    pub open: Autonomy,
    /// The assistant may click and type in pages (every action is asked, always).
    pub interact: bool,
    /// Sites the assistant never sees nor touches.
    pub blocked: Vec<String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: Provider::OpenAi,
            model: String::new(),
            base_url: String::new(),
            search: false,
            read: true,
            confirm_reads: true,
            open: Autonomy::Ask,
            interact: false,
            blocked: Vec::new(),
        }
    }
}

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        other => Err(format!("not a switch: {other}")),
    }
}

/// A model name is a word of letters, digits and `-_.:/` (`gpt-4.1`, `llama3.1:8b`,
/// `openrouter/auto`).
fn valid_model(model: &str) -> bool {
    model.len() <= MAX_MODEL
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'))
}

/// Whether `host` is this computer.
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// An address the assistant may be asked at: `https`, or plain `http` on this computer only (a
/// key or a conversation never crosses a network in the clear). No credentials, query or
/// fragment in it.
pub fn check_endpoint(text: &str) -> Result<Url, String> {
    let url = Url::parse(text.trim()).map_err(|_| "La dirección no es válida".to_owned())?;
    let host = url.host_str().unwrap_or_default().to_owned();
    if host.is_empty() {
        return Err("La dirección no es válida".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("La dirección no puede llevar usuario ni contraseña".to_owned());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("La dirección no puede llevar parámetros".to_owned());
    }
    match url.scheme() {
        "https" => Ok(url),
        "http" if is_loopback(&host) => Ok(url),
        "http" => Err(
            "Fuera de este equipo la dirección tiene que ser https: así no viajan la clave ni la conversación en claro".to_owned(),
        ),
        _ => Err("La dirección tiene que ser http o https".to_owned()),
    }
}

impl AiSettings {
    pub fn from_json(text: &str) -> Self {
        let mut s: Self = serde_json::from_str(text).unwrap_or_default();
        if !valid_model(&s.model) {
            s.model.clear();
        }
        if !s.base_url.is_empty() && check_endpoint(&s.base_url).is_err() {
            s.base_url.clear();
        }
        let mut blocked: Vec<String> = Vec::new();
        for h in std::mem::take(&mut s.blocked) {
            if let Some(h) = crate::boosts::host_of(&h) {
                if !blocked.contains(&h) {
                    blocked.push(h);
                }
            }
        }
        blocked.truncate(MAX_BLOCKED);
        s.blocked = blocked;
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The model that will be asked.
    pub fn model(&self) -> &str {
        if self.model.is_empty() {
            self.provider.default_model()
        } else {
            &self.model
        }
    }

    /// The address that will be asked, already checked.
    pub fn endpoint(&self) -> Result<Url, String> {
        let text = if self.provider.takes_address() && !self.base_url.is_empty() {
            self.base_url.as_str()
        } else {
            self.provider.default_base()
        };
        if text.is_empty() {
            return Err("Falta la dirección del servidor".to_owned());
        }
        check_endpoint(text)
    }

    /// Whether the conversation leaves this computer.
    pub fn is_remote(&self) -> bool {
        self.endpoint()
            .ok()
            .and_then(|u| u.host_str().map(|h| !is_loopback(h)))
            .unwrap_or(true)
    }

    /// Why the assistant cannot be asked yet, if it cannot.
    pub fn not_ready(&self, has_key: bool) -> Option<String> {
        if !self.enabled {
            return Some("El asistente está desactivado en Preferencias".to_owned());
        }
        if self.provider.needs_key() && !has_key {
            return Some(format!(
                "Falta la clave de {} en Preferencias → Inteligencia artificial",
                self.provider.short()
            ));
        }
        if self.model().is_empty() {
            return Some("Falta el nombre del modelo".to_owned());
        }
        self.endpoint().err()
    }

    /// Whether the assistant may look at `host` (a site the user excluded is off limits).
    pub fn host_allowed(&self, host: &str) -> bool {
        !crate::adblock::host_allowed(host, &self.blocked)
    }

    /// One change made on the preferences page. Checked before it is kept.
    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "enabled" => self.enabled = flag(value)?,
            "provider" => {
                self.provider = Provider::from_id(value).ok_or("unknown provider")?;
                // A model of one provider means nothing to another.
                self.model.clear();
            }
            "model" => {
                let v = value.trim();
                if !valid_model(v) {
                    return Err("not a model name".to_owned());
                }
                self.model = v.to_owned();
            }
            "base_url" => {
                let v = value.trim();
                if !v.is_empty() {
                    check_endpoint(v)?;
                }
                self.base_url = v.to_owned();
            }
            "search" => self.search = flag(value)?,
            "read" => self.read = flag(value)?,
            "confirm_reads" => self.confirm_reads = flag(value)?,
            "open" => {
                self.open = match value {
                    "ask" => Autonomy::Ask,
                    "auto" => Autonomy::Auto,
                    _ => return Err("unknown mode".to_owned()),
                }
            }
            "interact" => self.interact = flag(value)?,
            "blocked.add" => {
                let host = crate::boosts::host_of(value).ok_or("not a site")?;
                if !self.blocked.contains(&host) && self.blocked.len() < MAX_BLOCKED {
                    self.blocked.push(host);
                }
            }
            "blocked.remove" => self.blocked.retain(|h| h != value),
            other => return Err(format!("unknown setting: {other}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_starts_off_and_cautious() {
        let s = AiSettings::default();
        assert!(!s.enabled && !s.search && !s.interact);
        assert_eq!(s.open, Autonomy::Ask);
        assert!(s.confirm_reads);
        assert!(s.not_ready(true).is_some(), "disabled means not ready");
    }

    #[test]
    fn a_saved_file_cannot_smuggle_in_a_bad_address_or_model() {
        let s = AiSettings::from_json(
            r#"{"enabled":true,"provider":"custom","model":"x y;rm","base_url":"http://evil.example/v1","blocked":["Banco.com","https://banco.com/x","bad host"]}"#,
        );
        assert!(s.enabled);
        assert_eq!(s.model, "");
        assert_eq!(s.base_url, "");
        assert_eq!(s.blocked, vec!["banco.com".to_owned()]);
        assert!(AiSettings::from_json("not json") == AiSettings::default());
    }

    #[test]
    fn plain_http_is_only_for_this_computer() {
        assert!(check_endpoint("http://localhost:11434/v1").is_ok());
        assert!(check_endpoint("http://127.0.0.1:1234/v1").is_ok());
        assert!(check_endpoint("http://[::1]:8080/v1").is_ok());
        assert!(check_endpoint("http://192.168.1.20:11434/v1").is_err());
        assert!(check_endpoint("http://api.openai.com/v1").is_err());
        assert!(check_endpoint("https://api.example.com/v1").is_ok());
        assert!(check_endpoint("https://user:pw@api.example.com/v1").is_err());
        assert!(check_endpoint("https://api.example.com/v1?key=1").is_err());
        assert!(check_endpoint("ftp://example.com").is_err());
        assert!(check_endpoint("file:///etc/passwd").is_err());
        assert!(check_endpoint("javascript:alert(1)").is_err());
    }

    #[test]
    fn the_key_of_a_cloud_service_only_goes_to_its_own_address() {
        let mut s = AiSettings::default();
        s.apply("provider", "anthropic").unwrap();
        // Even with an address typed (left over from Ollama), a cloud service ignores it.
        s.base_url = "https://attacker.example/v1".to_owned();
        assert_eq!(
            s.endpoint().unwrap().as_str(),
            "https://api.anthropic.com/v1"
        );
        s.apply("provider", "custom").unwrap();
        assert_eq!(
            s.endpoint().unwrap().as_str(),
            "https://attacker.example/v1"
        );
    }

    #[test]
    fn ollama_is_local_and_the_others_are_not() {
        let mut s = AiSettings::default();
        assert!(s.is_remote());
        s.apply("provider", "ollama").unwrap();
        assert!(!s.is_remote());
        assert!(!s.provider.needs_key());
        assert_eq!(s.model(), "llama3.1");
    }

    #[test]
    fn readiness_says_what_is_missing() {
        let mut s = AiSettings::default();
        s.apply("enabled", "1").unwrap();
        assert!(s.not_ready(false).unwrap().contains("clave"));
        assert!(s.not_ready(true).is_none());
        s.apply("provider", "custom").unwrap();
        assert!(s.not_ready(false).unwrap().contains("modelo"));
        s.apply("model", "mi-modelo").unwrap();
        assert!(s.not_ready(false).unwrap().contains("dirección"));
        s.apply("base_url", "http://localhost:1234/v1").unwrap();
        assert!(s.not_ready(false).is_none());
    }

    #[test]
    fn changes_are_checked() {
        let mut s = AiSettings::default();
        assert!(s.apply("enabled", "maybe").is_err());
        assert!(s.apply("provider", "skynet").is_err());
        assert!(s.apply("model", "a b").is_err());
        assert!(s.apply("base_url", "http://10.0.0.1/v1").is_err());
        assert!(s.apply("open", "always").is_err());
        assert!(s.apply("blocked.add", "https://bad host/").is_err());
        assert!(s.apply("nope", "1").is_err());
        s.apply("blocked.add", "https://www.Banco.es/inicio")
            .unwrap();
        s.apply("blocked.add", "banco.es").unwrap();
        assert_eq!(s.blocked, vec!["banco.es".to_owned()]);
        assert!(!s.host_allowed("www.banco.es"));
        assert!(!s.host_allowed("login.banco.es"));
        assert!(s.host_allowed("example.com"));
        s.apply("blocked.remove", "banco.es").unwrap();
        assert!(s.host_allowed("banco.es"));
    }

    #[test]
    fn changing_provider_forgets_the_model() {
        let mut s = AiSettings::default();
        s.apply("model", "gpt-4o").unwrap();
        s.apply("provider", "gemini").unwrap();
        assert_eq!(s.model(), "gemini-2.5-flash");
    }
}
