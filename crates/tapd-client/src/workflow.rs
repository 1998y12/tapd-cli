use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result, field_string, records, value_as_string};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendField {
    pub name: String,
    pub required: bool,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub previous: String,
    pub next: String,
    pub append_fields: Vec<AppendField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionPlan {
    pub current: String,
    pub target: String,
    pub transitions: Vec<Transition>,
}

/// Build and validate the shortest transition route from a story's current
/// state to a configured target state.
///
/// # Errors
///
/// Returns an error when workflow metadata is malformed, either endpoint is
/// unknown or ambiguous, no route exists, or a transition requires an
/// append-field value that is unavailable.
pub fn plan_transition(
    status_map_data: &Value,
    transitions_data: &Value,
    current: &str,
    target: &str,
    existing_story: &Value,
    provided_fields: &BTreeMap<String, String>,
) -> Result<TransitionPlan> {
    let statuses = parse_status_map(status_map_data)?;
    let current = resolve_status(current, &statuses)?;
    let target = resolve_status(target, &statuses)?;
    if current == target {
        return Ok(TransitionPlan {
            current,
            target,
            transitions: Vec::new(),
        });
    }

    let transitions = parse_transitions(transitions_data, &statuses)?;
    let route = shortest_route(&transitions, &current, &target)
        .ok_or_else(|| Error::Workflow(format!("no path exists from {current:?} to {target:?}")))?;
    validate_required_fields(&route, existing_story, provided_fields)?;

    Ok(TransitionPlan {
        current,
        target,
        transitions: route,
    })
}

fn parse_status_map(value: &Value) -> Result<BTreeMap<String, String>> {
    let Value::Object(map) = value else {
        return Err(Error::Workflow("status map is not an object".into()));
    };
    map.iter()
        .map(|(key, value)| {
            value_as_string(value)
                .map(|label| (key.clone(), label))
                .ok_or_else(|| Error::Workflow(format!("invalid status label for {key}")))
        })
        .collect()
}

fn parse_transitions(
    value: &Value,
    statuses: &BTreeMap<String, String>,
) -> Result<Vec<Transition>> {
    records(value, "WorkflowTransition")?
        .into_iter()
        .map(|item| {
            let previous = field_any(&item, &["StepPrevious", "step_previous"])
                .ok_or_else(|| Error::Workflow("transition has no previous state".into()))?;
            let next = field_any(&item, &["StepNext", "step_next"])
                .ok_or_else(|| Error::Workflow("transition has no next state".into()))?;
            let append_fields = item
                .get("Appendfield")
                .or_else(|| item.get("appendfield"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_append_field)
                .collect();
            Ok(Transition {
                name: field_any(&item, &["Name", "name"]).unwrap_or_default(),
                previous: resolve_status(&previous, statuses)?,
                next: resolve_status(&next, statuses)?,
                append_fields,
            })
        })
        .collect()
}

fn parse_append_field(value: &Value) -> Option<AppendField> {
    let name = field_any(value, &["FieldName", "field_name"])?;
    let required = field_any(value, &["Notnull", "notnull"])
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "yes" | "1" | "true"));
    let has_default = value
        .get("DefaultValue")
        .or_else(|| value.get("default_value"))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values.iter().any(|value| {
                field_any(value, &["Value", "value"]).is_some_and(|value| !value.is_empty())
            })
        });
    Some(AppendField {
        name,
        required,
        has_default,
    })
}

fn resolve_status(value: &str, statuses: &BTreeMap<String, String>) -> Result<String> {
    if statuses.contains_key(value) {
        return Ok(value.to_owned());
    }
    let matches: Vec<_> = statuses
        .iter()
        .filter(|(_, label)| label.as_str() == value)
        .map(|(code, _)| code.clone())
        .collect();
    match matches.as_slice() {
        [code] => Ok(code.clone()),
        [] => Err(Error::Workflow(format!("unknown status {value:?}"))),
        _ => Err(Error::Workflow(format!(
            "status label {value:?} is ambiguous"
        ))),
    }
}

fn shortest_route(
    transitions: &[Transition],
    current: &str,
    target: &str,
) -> Option<Vec<Transition>> {
    let mut queue = VecDeque::from([current.to_owned()]);
    let mut visited = HashSet::from([current.to_owned()]);
    let mut parent: HashMap<String, (String, usize)> = HashMap::new();

    while let Some(state) = queue.pop_front() {
        for (index, transition) in transitions.iter().enumerate() {
            if transition.previous != state || visited.contains(&transition.next) {
                continue;
            }
            parent.insert(transition.next.clone(), (state.clone(), index));
            if transition.next == target {
                let mut route = Vec::new();
                let mut cursor = target.to_owned();
                while cursor != current {
                    let (previous, transition_index) = parent.get(&cursor)?.clone();
                    route.push(transitions[transition_index].clone());
                    cursor = previous;
                }
                route.reverse();
                return Some(route);
            }
            visited.insert(transition.next.clone());
            queue.push_back(transition.next.clone());
        }
    }
    None
}

fn validate_required_fields(
    route: &[Transition],
    existing_story: &Value,
    provided_fields: &BTreeMap<String, String>,
) -> Result<()> {
    let missing: Vec<_> = route
        .iter()
        .flat_map(|transition| &transition.append_fields)
        .filter(|field| field.required && !field.has_default)
        .filter(|field| {
            provided_fields
                .get(&field.name)
                .is_none_or(String::is_empty)
                && field_string(existing_story, &field.name).is_none_or(|value| value.is_empty())
        })
        .map(|field| field.name.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::Workflow(format!(
            "required transition fields are missing: {}",
            missing.join(", ")
        )))
    }
}

fn field_any(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(value_as_string))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn plans_multi_step_route_using_labels() {
        let status_map = json!({"open": "新建", "review": "待校核", "done": "已实现"});
        let transitions = json!([
            {"Name":"送校核","StepPrevious":"open","StepNext":"review","Appendfield":[]},
            {"Name":"完成","StepPrevious":"review","StepNext":"done","Appendfield":[]}
        ]);
        let plan = plan_transition(
            &status_map,
            &transitions,
            "新建",
            "已实现",
            &json!({}),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(plan.transitions.len(), 2);
        assert_eq!(plan.transitions[1].next, "done");
    }

    #[test]
    fn rejects_missing_required_append_field_before_writes() {
        let status_map = json!({"review": "待校核", "done": "已实现"});
        let transitions = json!([{
            "StepPrevious":"review",
            "StepNext":"done",
            "Appendfield":[{"FieldName":"custom_field_17","Notnull":"yes","DefaultValue":[]}]
        }]);
        let error = plan_transition(
            &status_map,
            &transitions,
            "review",
            "done",
            &json!({}),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("custom_field_17"));
    }
}
