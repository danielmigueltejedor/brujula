//! Scheduled tasks: something the user asked the assistant to do on its own at a time, such as
//! looking at a site every morning. A task says what to do, on which sites and when; it never
//! says how to sign in or what to click. While it runs it can only read the sites it names
//! (see `policy::decide`), and everything it answers is kept for the user to read.
//!
//! Time here is plain numbers, so the rules are tested without a clock: `now` is in seconds
//! since 1970 and `offset` is the local time's distance from UTC in seconds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEDULE: &str = "schedule_task";

/// Tasks a profile can hold.
pub const MAX_TASKS: usize = 12;
/// Sites one task may read.
pub const MAX_HOSTS: usize = 5;
const MAX_NAME: usize = 60;
const MAX_PROMPT: usize = 1500;
const MAX_RESULT: usize = 4000;
const DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "every", rename_all = "lowercase")]
pub enum Schedule {
    /// Every `n` hours (1 to 24) after the last run.
    Hours { n: u32 },
    /// Every day at this local time.
    Day { h: u8, m: u8 },
    /// On these days (bit 0 is Monday) at this local time.
    Week { days: u8, h: u8, m: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: u32,
    pub name: String,
    /// What to do, in the user's words.
    pub prompt: String,
    /// The only sites it may open and read.
    pub hosts: Vec<String>,
    pub schedule: Schedule,
    pub enabled: bool,
    /// When it last ran, in seconds since 1970 (0 if never).
    pub last_run: u64,
    /// What it answered the last time.
    pub last_result: String,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            prompt: String::new(),
            hosts: Vec::new(),
            schedule: Schedule::Day { h: 8, m: 0 },
            enabled: true,
            last_run: 0,
            last_result: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tasks {
    list: Vec<Task>,
}

fn one_line(text: &str, max: usize) -> String {
    let t: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    t.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

impl Schedule {
    fn valid(self) -> bool {
        match self {
            Schedule::Hours { n } => (1..=24).contains(&n),
            Schedule::Day { h, m } => h < 24 && m < 60,
            Schedule::Week { days, h, m } => days & 0x7f != 0 && days < 128 && h < 24 && m < 60,
        }
    }

    /// The last moment this schedule asked for a run at or before `now`, as seconds since 1970;
    /// `None` for one that has no fixed moments (every n hours).
    fn last_slot(self, now: u64, offset: i64) -> Option<u64> {
        let local = i64::try_from(now).ok()? + offset;
        let day = local.div_euclid(DAY);
        let at = |h: u8, m: u8| i64::from(h) * 3600 + i64::from(m) * 60;
        let slot = match self {
            Schedule::Hours { .. } => return None,
            Schedule::Day { h, m } => {
                let today = day * DAY + at(h, m);
                if today <= local {
                    today
                } else {
                    today - DAY
                }
            }
            Schedule::Week { days, h, m } => (0..8).find_map(|back| {
                let d = day - back;
                // 1 January 1970 was a Thursday.
                let weekday = (d + 3).rem_euclid(7);
                let slot = d * DAY + at(h, m);
                (days >> weekday & 1 == 1 && slot <= local).then_some(slot)
            })?,
        };
        u64::try_from(slot - offset).ok()
    }

    /// Whether it is time to run, given when it last did.
    pub fn due(self, now: u64, offset: i64, last_run: u64) -> bool {
        match self {
            Schedule::Hours { n } => now >= last_run + u64::from(n) * 3600,
            _ => self
                .last_slot(now, offset)
                .is_some_and(|slot| slot > last_run),
        }
    }

    /// The schedule in words.
    pub fn describe(self) -> String {
        const DAYS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
        match self {
            Schedule::Hours { n: 1 } => "cada hora".to_owned(),
            Schedule::Hours { n } => format!("cada {n} horas"),
            Schedule::Day { h, m } => format!("cada día a las {h:02}:{m:02}"),
            Schedule::Week { days, h, m } => {
                let names: Vec<&str> = (0..7)
                    .filter(|d| days >> d & 1 == 1)
                    .map(|d| DAYS[d])
                    .collect();
                format!("{} a las {h:02}:{m:02}", names.join(", "))
            }
        }
    }
}

fn parse_time(text: &str) -> Result<(u8, u8), String> {
    let (h, m) = text.split_once(':').ok_or("La hora va como HH:MM")?;
    let h: u8 = h.trim().parse().map_err(|_| "La hora no es válida")?;
    let m: u8 = m.trim().parse().map_err(|_| "La hora no es válida")?;
    if h > 23 || m > 59 {
        return Err("La hora no es válida".to_owned());
    }
    Ok((h, m))
}

fn parse_day(text: &str) -> Option<u8> {
    let t = text.trim().to_lowercase();
    let t = t.trim_end_matches('.');
    [
        "lun", "mar", "mie", "mié", "jue", "vie", "sab", "sáb", "dom",
    ]
    .iter()
    .position(|d| t.starts_with(d))
    .map(|i| match i {
        0 => 0,
        1 => 1,
        2 | 3 => 2,
        4 => 3,
        5 => 4,
        6 | 7 => 5,
        _ => 6,
    })
}

/// A task as the model proposes it. Nothing is kept until the user says yes.
pub fn parse(args: &Value, now: u64) -> Result<Task, String> {
    let name = one_line(args["name"].as_str().unwrap_or_default(), MAX_NAME);
    if name.is_empty() {
        return Err("Falta «name»".to_owned());
    }
    let prompt = one_line(args["prompt"].as_str().unwrap_or_default(), MAX_PROMPT);
    if prompt.is_empty() {
        return Err("Falta «prompt»: qué tiene que hacer la tarea".to_owned());
    }
    let mut hosts: Vec<String> = Vec::new();
    for site in args["sites"].as_array().into_iter().flatten() {
        let h = site
            .as_str()
            .and_then(crate::boosts::host_of)
            .ok_or("Cada sitio es una dirección como «agora.upm.es»")?;
        if !hosts.contains(&h) {
            hosts.push(h);
        }
    }
    if hosts.is_empty() || hosts.len() > MAX_HOSTS {
        return Err(format!("«sites» necesita entre 1 y {MAX_HOSTS} sitios"));
    }
    let when = &args["when"];
    let schedule = match when["every"].as_str().unwrap_or_default() {
        "hours" => Schedule::Hours {
            n: u32::try_from(when["n"].as_u64().unwrap_or(0)).unwrap_or(0),
        },
        "day" => {
            let (h, m) = parse_time(when["at"].as_str().unwrap_or_default())?;
            Schedule::Day { h, m }
        }
        "week" => {
            let (h, m) = parse_time(when["at"].as_str().unwrap_or_default())?;
            let mut days = 0u8;
            for d in when["days"].as_array().into_iter().flatten() {
                days |= 1 << d.as_str().and_then(parse_day).ok_or("Día no válido")?;
            }
            Schedule::Week { days, h, m }
        }
        _ => return Err("«when.every» es hours, day o week".to_owned()),
    };
    if !schedule.valid() {
        return Err("El horario no es válido".to_owned());
    }
    Ok(Task {
        name,
        prompt,
        hosts,
        schedule,
        // It starts counting now: a task made at noon does not run for a slot that was at eight.
        last_run: now,
        ..Task::default()
    })
}

impl Task {
    /// The task in words, for the question the user is asked before it is kept.
    pub fn describe(&self) -> String {
        format!(
            "«{}»\nQué hace: {}\nCuándo: {}\nSolo puede abrir y leer: {}\nNo pulsa, no escribe, no inicia sesión y no ve tus pestañas.",
            self.name,
            self.prompt,
            self.schedule.describe(),
            self.hosts.join(", ")
        )
    }
}

impl Tasks {
    pub fn from_json(text: &str) -> Self {
        let mut t: Self = serde_json::from_str(text).unwrap_or_default();
        t.list.truncate(MAX_TASKS);
        let mut seen: Vec<u32> = Vec::new();
        t.list.retain_mut(|task| {
            task.name = one_line(&task.name, MAX_NAME);
            task.prompt = one_line(&task.prompt, MAX_PROMPT);
            task.last_result.truncate(
                task.last_result
                    .char_indices()
                    .nth(MAX_RESULT)
                    .map_or(task.last_result.len(), |(i, _)| i),
            );
            task.hosts = task
                .hosts
                .iter()
                .filter_map(|h| crate::boosts::host_of(h))
                .take(MAX_HOSTS)
                .collect();
            let fine = task.id != 0
                && !seen.contains(&task.id)
                && !task.name.is_empty()
                && !task.prompt.is_empty()
                && !task.hosts.is_empty()
                && task.schedule.valid();
            if fine {
                seen.push(task.id);
            }
            fine
        });
        t
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn all(&self) -> &[Task] {
        &self.list
    }

    pub fn get(&self, id: u32) -> Option<&Task> {
        self.list.iter().find(|t| t.id == id)
    }

    /// Keeps a task and returns its id, if there is room.
    pub fn add(&mut self, mut task: Task) -> Option<u32> {
        if self.list.len() >= MAX_TASKS {
            return None;
        }
        task.id = self.list.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let id = task.id;
        self.list.push(task);
        Some(id)
    }

    pub fn remove(&mut self, id: u32) {
        self.list.retain(|t| t.id != id);
    }

    pub fn set_enabled(&mut self, id: u32, on: bool) {
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.enabled = on;
        }
    }

    /// Writes down that a task is starting now, so no other window starts it too.
    pub fn start(&mut self, id: u32, now: u64) {
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.last_run = now;
        }
    }

    /// Writes down that a task ran and what it answered.
    pub fn record(&mut self, id: u32, now: u64, result: &str) {
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.last_run = now;
            t.last_result = result.chars().take(MAX_RESULT).collect();
        }
    }

    /// The first enabled task whose time has come.
    pub fn due(&self, now: u64, offset: i64) -> Option<u32> {
        self.list
            .iter()
            .find(|t| t.enabled && t.schedule.due(now, offset, t.last_run))
            .map(|t| t.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Friday 2026-11-13 10:00 UTC; Madrid is UTC+1 then.
    const FRI_10: u64 = 1_794_564_000;
    const HOUR: i64 = 3600;

    fn proposal() -> Value {
        json!({
            "name": "Ágora\n",
            "prompt": "Mira las asignaturas y dime si hay algo nuevo",
            "sites": ["https://agora.upm.es/login", "agora.upm.es", "campus.example"],
            "when": {"every": "day", "at": "08:30"}
        })
    }

    #[test]
    fn a_proposal_is_read_and_cleaned() {
        let t = parse(&proposal(), FRI_10).unwrap();
        assert_eq!(t.name, "Ágora");
        assert_eq!(t.hosts, vec!["agora.upm.es", "campus.example"]);
        assert_eq!(t.schedule, Schedule::Day { h: 8, m: 30 });
        assert_eq!(t.last_run, FRI_10);
        assert!(t.describe().contains("agora.upm.es"));
    }

    #[test]
    fn a_proposal_without_sites_or_with_odd_times_is_refused() {
        let with = |key: &str, value: Value| {
            let mut p = proposal();
            p[key] = value;
            parse(&p, 0)
        };
        assert!(with("sites", json!([])).is_err());
        assert!(with("sites", json!([5])).is_err());
        assert!(with(
            "sites",
            json!([
                "a.example",
                "b.example",
                "c.example",
                "d.example",
                "e.example",
                "f.example"
            ])
        )
        .is_err());
        assert!(with("when", json!({"every": "day", "at": "25:00"})).is_err());
        assert!(with("when", json!({"every": "hours", "n": 0})).is_err());
        assert!(with("when", json!({"every": "hours", "n": 25})).is_err());
        assert!(with("when", json!({"every": "week", "days": [], "at": "08:00"})).is_err());
        assert!(with(
            "when",
            json!({"every": "week", "days": ["funday"], "at": "08:00"})
        )
        .is_err());
        assert!(with("when", json!({"every": "minutes"})).is_err());
        assert!(with("prompt", json!("   ")).is_err());
        assert!(with("name", json!("")).is_err());
        let week = with(
            "when",
            json!({"every": "week", "days": ["lun", "viernes"], "at": "08:00"}),
        )
        .unwrap();
        assert_eq!(
            week.schedule,
            Schedule::Week {
                days: 0b0010001,
                h: 8,
                m: 0
            }
        );
    }

    #[test]
    fn a_daily_task_runs_once_per_slot() {
        let s = Schedule::Day { h: 8, m: 30 };
        let off = HOUR;
        // Friday 07:00 local: the last slot was Thursday 08:30, which already ran.
        let early = FRI_10 - 3 * 3600;
        let thursday_run = (FRI_10 as i64 - 26 * HOUR + 30 * 60) as u64;
        assert!(!s.due(early, off, thursday_run));
        // Friday 09:00 local: the Friday slot has passed.
        let nine = FRI_10 - 2 * 3600;
        assert!(s.due(nine, off, thursday_run));
        // Once it ran, it is not due again the same day.
        assert!(!s.due(nine + 600, off, nine));
        // A task made at 09:00 does not run for the 08:30 slot that came before it.
        assert!(!s.due(nine + 3600, off, nine));
        // The next morning it does.
        assert!(s.due(nine + DAY as u64, off, nine));
    }

    #[test]
    fn a_weekly_task_only_runs_on_its_days() {
        // Mondays and Fridays at 08:30 local.
        let s = Schedule::Week {
            days: 0b0010001,
            h: 8,
            m: 30,
        };
        let off = HOUR;
        let fri_nine = FRI_10 - 2 * 3600;
        let last_mon = fri_nine - 4 * DAY as u64;
        assert!(s.due(fri_nine, off, last_mon));
        // Saturday: the slot is still Friday's, which already ran.
        assert!(!s.due(fri_nine + DAY as u64, off, fri_nine));
        // Wednesday after a Monday run: nothing.
        assert!(!s.due(fri_nine - 2 * DAY as u64, off, last_mon));
        // The next Monday it is due again.
        assert!(s.due(fri_nine + 3 * DAY as u64, off, fri_nine));
    }

    #[test]
    fn hourly_tasks_count_from_the_last_run() {
        let s = Schedule::Hours { n: 6 };
        assert!(!s.due(1000 + 5 * 3600, 0, 1000));
        assert!(s.due(1000 + 6 * 3600, 0, 1000));
    }

    #[test]
    fn the_store_keeps_only_sound_tasks() {
        let mut tasks = Tasks::default();
        let id = tasks.add(parse(&proposal(), FRI_10).unwrap()).unwrap();
        assert_eq!(id, 1);
        tasks.record(id, FRI_10 + 5, "Hay 2 avisos nuevos");
        let back = Tasks::from_json(&tasks.to_json());
        assert_eq!(back, tasks);
        let broken = r#"{"list":[{"id":0,"name":"x","prompt":"y","hosts":["a.example"]},{"id":2,"name":"ok","prompt":"y","hosts":["a.example"]},{"id":2,"name":"dup","prompt":"y","hosts":["a.example"]},{"id":3,"name":"sin sitios","prompt":"y","hosts":[]}]}"#;
        let t = Tasks::from_json(broken);
        assert_eq!(t.all().len(), 1);
        assert_eq!(t.all()[0].name, "ok");
        assert!(Tasks::from_json("no json").all().is_empty());
    }

    #[test]
    fn due_picks_enabled_tasks_only() {
        let mut tasks = Tasks::default();
        let mut t = parse(&proposal(), 0).unwrap();
        t.schedule = Schedule::Hours { n: 1 };
        let id = tasks.add(t).unwrap();
        assert_eq!(tasks.due(10_000, 0), Some(id));
        tasks.set_enabled(id, false);
        assert_eq!(tasks.due(10_000, 0), None);
        tasks.set_enabled(id, true);
        tasks.record(id, 10_000, "");
        assert_eq!(tasks.due(10_100, 0), None);
        // Starting counts as having run, so a second window does not take it too.
        tasks.start(id, 10_000 + 4000);
        assert_eq!(tasks.due(10_000 + 3600, 0), None);
    }
}
