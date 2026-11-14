//! Plans for tidying the tabs: pin, move, put in a folder, send to a space, close. The model
//! sends the whole plan in one call, so it can be read, checked and shown to the user before any
//! of it happens. Nothing here touches a tab: the app carries the plan out.

use serde_json::Value;

pub const ORGANIZE: &str = "organize_tabs";

/// Steps in one plan.
pub const MAX_OPS: usize = 40;
const MAX_NAME: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Pin(u64),
    Unpin(u64),
    /// The tab goes right before (or after) another one.
    Move {
        tab: u64,
        next_to: u64,
        after: bool,
    },
    /// Puts the tabs in the folder with this name, which is made if it does not exist.
    Folder {
        name: String,
        tabs: Vec<u64>,
    },
    /// Sends the tab to the space with this name, which must exist.
    Space {
        tab: u64,
        space: String,
    },
    Close(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<Op>,
}

fn tab_id(v: &Value, key: &str) -> Result<u64, String> {
    v[key]
        .as_u64()
        .ok_or_else(|| format!("Falta «{key}» (el número de una pestaña)"))
}

/// A name for a folder or a space: one line, no control characters, not too long.
fn name(v: &Value, key: &str) -> Result<String, String> {
    let text: String = v[key]
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("Falta «{key}»"));
    }
    Ok(text.chars().take(MAX_NAME).collect())
}

/// Reads the arguments of the call. Unknown steps are refused, not skipped: a plan is either
/// understood or it is not done.
pub fn parse(args: &Value) -> Result<Plan, String> {
    let list = args["actions"]
        .as_array()
        .ok_or("Falta «actions»: la lista de pasos")?;
    if list.is_empty() {
        return Err("La lista de pasos está vacía".to_owned());
    }
    if list.len() > MAX_OPS {
        return Err(format!("Demasiados pasos de una vez (máximo {MAX_OPS})"));
    }
    let mut ops = Vec::with_capacity(list.len());
    for step in list {
        let op = match step["do"].as_str().unwrap_or_default() {
            "pin" => Op::Pin(tab_id(step, "tab")?),
            "unpin" => Op::Unpin(tab_id(step, "tab")?),
            "close" => Op::Close(tab_id(step, "tab")?),
            "move" => {
                let (key, after) = match (step["before"].as_u64(), step["after"].as_u64()) {
                    (Some(_), None) => ("before", false),
                    (None, Some(_)) => ("after", true),
                    _ => return Err("«move» lleva «before» o «after» (una pestaña)".to_owned()),
                };
                Op::Move {
                    tab: tab_id(step, "tab")?,
                    next_to: tab_id(step, key)?,
                    after,
                }
            }
            "folder" => {
                let tabs: Vec<u64> = step["tabs"]
                    .as_array()
                    .ok_or("Falta «tabs» en «folder»")?
                    .iter()
                    .filter_map(Value::as_u64)
                    .collect();
                if tabs.is_empty() || tabs.len() > MAX_OPS {
                    return Err("«folder» necesita entre 1 y 40 pestañas".to_owned());
                }
                Op::Folder {
                    name: name(step, "name")?,
                    tabs,
                }
            }
            "space" => Op::Space {
                tab: tab_id(step, "tab")?,
                space: name(step, "space")?,
            },
            other => return Err(format!("Paso desconocido: «{other}»")),
        };
        ops.push(op);
    }
    Ok(Plan { ops })
}

impl Plan {
    /// The tabs the plan closes, once each.
    pub fn closes(&self) -> Vec<u64> {
        let mut v: Vec<u64> = Vec::new();
        for op in &self.ops {
            if let Op::Close(id) = op {
                if !v.contains(id) {
                    v.push(*id);
                }
            }
        }
        v
    }

    /// Every tab the plan touches.
    pub fn tabs(&self) -> Vec<u64> {
        let mut v: Vec<u64> = Vec::new();
        for op in &self.ops {
            let ids: Vec<u64> = match op {
                Op::Pin(t) | Op::Unpin(t) | Op::Close(t) => vec![*t],
                Op::Move { tab, next_to, .. } => vec![*tab, *next_to],
                Op::Folder { tabs, .. } => tabs.clone(),
                Op::Space { tab, .. } => vec![*tab],
            };
            for id in ids {
                if !v.contains(&id) {
                    v.push(id);
                }
            }
        }
        v
    }

    /// The plan in words, one line per step, for the question the user is asked.
    pub fn describe(&self, title: impl Fn(u64) -> String) -> Vec<String> {
        let t = |id: u64| format!("«{}»", title(id));
        self.ops
            .iter()
            .map(|op| match op {
                Op::Pin(id) => format!("Fijar {}", t(*id)),
                Op::Unpin(id) => format!("Soltar {}", t(*id)),
                Op::Close(id) => format!("Cerrar {}", t(*id)),
                Op::Move {
                    tab,
                    next_to,
                    after,
                } => format!(
                    "Poner {} {} {}",
                    t(*tab),
                    if *after { "después de" } else { "antes de" },
                    t(*next_to)
                ),
                Op::Folder { name, tabs } => format!(
                    "Carpeta «{name}»: {}",
                    tabs.iter().map(|id| t(*id)).collect::<Vec<_>>().join(", ")
                ),
                Op::Space { tab, space } => format!("Pasar {} al espacio «{space}»", t(*tab)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plan_is_read_step_by_step() {
        let p = parse(&json!({"actions": [
            {"do": "pin", "tab": 1},
            {"do": "move", "tab": 2, "before": 1},
            {"do": "folder", "name": "  Viajes\n", "tabs": [3, 4]},
            {"do": "space", "tab": 5, "space": "Trabajo"},
            {"do": "close", "tab": 6},
            {"do": "close", "tab": 6}
        ]}))
        .unwrap();
        assert_eq!(p.ops.len(), 6);
        assert_eq!(
            p.ops[2],
            Op::Folder {
                name: "Viajes".into(),
                tabs: vec![3, 4]
            }
        );
        assert_eq!(p.closes(), vec![6]);
        assert_eq!(p.tabs(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn what_is_not_understood_is_not_done() {
        for bad in [
            json!({}),
            json!({"actions": []}),
            json!({"actions": [{"do": "format_disk"}]}),
            json!({"actions": [{"do": "pin"}]}),
            json!({"actions": [{"do": "move", "tab": 1}]}),
            json!({"actions": [{"do": "move", "tab": 1, "before": 2, "after": 3}]}),
            json!({"actions": [{"do": "folder", "name": "", "tabs": [1]}]}),
            json!({"actions": [{"do": "folder", "name": "x", "tabs": []}]}),
        ] {
            assert!(parse(&bad).is_err(), "{bad}");
        }
        let many: Vec<Value> = (0..=MAX_OPS)
            .map(|i| json!({"do": "pin", "tab": i}))
            .collect();
        assert!(parse(&json!({"actions": many})).is_err());
    }

    #[test]
    fn the_plan_is_shown_in_words() {
        let p = parse(&json!({"actions": [
            {"do": "close", "tab": 7},
            {"do": "folder", "name": "Viajes", "tabs": [1, 2]}
        ]}))
        .unwrap();
        let lines = p.describe(|id| format!("pág {id}"));
        assert_eq!(lines[0], "Cerrar «pág 7»");
        assert_eq!(lines[1], "Carpeta «Viajes»: «pág 1», «pág 2»");
    }

    #[test]
    fn names_lose_their_control_characters_and_their_length() {
        let p = parse(
            &json!({"actions": [{"do": "folder", "name": "a\u{7}b".repeat(40), "tabs": [1]}]}),
        )
        .unwrap();
        let Op::Folder { name, .. } = &p.ops[0] else {
            panic!()
        };
        assert!(name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control));
    }
}
