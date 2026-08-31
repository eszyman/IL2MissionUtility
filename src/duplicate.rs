//! Template duplication: unique Index reallocation and MCU pointer reconnection.

use std::collections::HashMap;

use crate::ast::Il2Entity;

/// Property keys that store a single entity Index (MCU logic links).
const ID_POINTER_KEYS: &[&str] = &["LinkTrId", "MisObjID", "TarId", "CmdId"];

/// Recursively clone `template`, assigning sequentially higher unique integers
/// to every `Index`. Returns the clone and a map of old Index → new Index.
pub fn reallocate_ids(
    template: &Il2Entity,
    next_id: &mut i32,
) -> (Il2Entity, HashMap<i32, i32>) {
    let mut map = HashMap::new();
    let mut clone = template.clone();
    reallocate_recursive(&mut clone, next_id, &mut map);
    (clone, map)
}

fn reallocate_recursive(
    entity: &mut Il2Entity,
    next_id: &mut i32,
    map: &mut HashMap<i32, i32>,
) {
    if let Some(old) = entity.index {
        let new = *next_id;
        *next_id += 1;
        map.insert(old, new);
        entity.index = Some(new);
        entity.set_property("Index", new.to_string());
    }
    for child in &mut entity.children {
        reallocate_recursive(child, next_id, map);
    }
}

/// Walk `targets` and `objects` (and known ID pointer properties), replacing
/// old Indexes with the values recorded in `map`. IDs not present in the map
/// are left unchanged so external MCU links stay intact.
pub fn reconnect_pointers(entity: &mut Il2Entity, map: &HashMap<i32, i32>) {
    for id in &mut entity.targets {
        if let Some(&new) = map.get(id) {
            *id = new;
        }
    }
    for id in &mut entity.objects {
        if let Some(&new) = map.get(id) {
            *id = new;
        }
    }
    sync_link_array_property(entity, "Targets", &entity.targets.clone());
    sync_link_array_property(entity, "Objects", &entity.objects.clone());

    for (key, value) in &mut entity.properties {
        if !ID_POINTER_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(old) = value.parse::<i32>() {
            if let Some(&new) = map.get(&old) {
                *value = new.to_string();
            }
        }
    }

    for child in &mut entity.children {
        reconnect_pointers(child, map);
    }
}

fn sync_link_array_property(entity: &mut Il2Entity, key: &str, ids: &[i32]) {
    if entity.property(key).is_none() {
        return;
    }
    entity.set_property(key, crate::ast::format_int_array(ids));
}

/// Clone `template`, reallocate Indexes starting at `next_id`, then reconnect
/// internal MCU pointers.
pub fn duplicate_template(
    template: &Il2Entity,
    next_id: &mut i32,
) -> (Il2Entity, HashMap<i32, i32>) {
    let (mut clone, map) = reallocate_ids(template, next_id);
    reconnect_pointers(&mut clone, &map);
    (clone, map)
}

/// Produce `count` copies of `template` with unique Indexes and intact MCU links.
///
/// The first copy keeps the template's original Indexes. Additional copies are
/// reallocated from `template.max_index() + 1`. When `count > 1`, copies are
/// wrapped in a parent Group.
#[allow(dead_code)]
pub fn generate_groups(template: &Il2Entity, count: usize) -> Il2Entity {
    let count = count.max(1);
    let mut next_id = template.max_index().saturating_add(1);
    let mut copies = Vec::with_capacity(count);
    copies.push(template.clone());
    for _ in 1..count {
        let (clone, _) = duplicate_template(template, &mut next_id);
        copies.push(clone);
    }
    if count == 1 {
        return copies.pop().unwrap();
    }
    let wrapper_id = next_id;
    let mut wrapper = Il2Entity::new("Group");
    wrapper.index = Some(wrapper_id);
    wrapper.set_property("Name", "\"Generated Groups\"");
    wrapper.set_property("Index", wrapper_id.to_string());
    wrapper.set_property("Desc", "\"\"");
    wrapper.children = copies;
    wrapper
}

/// Apply GUI overrides: `Country` on Plane/Vehicle, Script/Model on Plane.
pub fn apply_overrides(entity: &mut Il2Entity, aircraft_asset_path: &str, coalition: i32) {
    if entity.block_type == "Plane"
        || entity.block_type == "Vehicle"
        || entity.block_type == "Ship"
    {
        entity.set_property("Country", coalition.to_string());
    }
    if entity.block_type == "Plane" {
        if let Some((script, model)) = aircraft_script_and_model(aircraft_asset_path) {
            entity.set_property("Script", format!("\"{script}\""));
            entity.set_property("Model", format!("\"{model}\""));
        }
    }
    for child in &mut entity.children {
        apply_overrides(child, aircraft_asset_path, coalition);
    }
}

fn aircraft_script_and_model(path: &str) -> Option<(String, String)> {
    let path = path.trim().replace('/', "\\");
    if path.is_empty() {
        return None;
    }
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(path.as_str())
        .trim_end_matches(".txt")
        .trim_end_matches(".mgm")
        .trim_end_matches(".MGM")
        .trim_end_matches(".TXT")
        .to_string();
    if name.is_empty() {
        return None;
    }
    let script = if path.to_ascii_lowercase().ends_with(".txt") && path.contains('\\') {
        path
    } else {
        format!("LuaScripts\\WorldObjects\\Planes\\{name}.txt")
    };
    let model = format!("graphics\\planes\\{name}\\{name}.mgm");
    Some((script, model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_entity;

    fn sample_linked_group() -> Il2Entity {
        let src = r#"Group
{
  Name = "Pair";
  Index = 10;
  Vehicle
  {
    Name = "car";
    Index = 20;
    LinkTrId = 21;
    Country = 503;
  }
  MCU_TR_Entity
  {
    Index = 21;
    Name = "car entity";
    Targets = [20];
    Objects = [21];
    MisObjID = 20;
    OnEvents
    {
      OnEvent
      {
        Type = 4;
        TarId = 10;
      }
    }
  }
}"#;
        parse_entity(src).expect("sample").1
    }

    #[test]
    fn reallocate_assigns_sequential_indexes() {
        let template = sample_linked_group();
        let mut next_id = 100;
        let (clone, map) = reallocate_ids(&template, &mut next_id);

        assert_eq!(clone.index, Some(100));
        assert_eq!(clone.children[0].index, Some(101));
        assert_eq!(clone.children[1].index, Some(102));
        assert_eq!(next_id, 103);

        assert_eq!(map.get(&10), Some(&100));
        assert_eq!(map.get(&20), Some(&101));
        assert_eq!(map.get(&21), Some(&102));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn reallocate_does_not_mutate_template() {
        let template = sample_linked_group();
        let mut next_id = 100;
        let _ = reallocate_ids(&template, &mut next_id);
        assert_eq!(template.index, Some(10));
        assert_eq!(template.children[0].index, Some(20));
    }

    #[test]
    fn reconnect_rewrites_targets_and_objects() {
        let template = sample_linked_group();
        let mut next_id = 100;
        let (mut clone, map) = reallocate_ids(&template, &mut next_id);
        reconnect_pointers(&mut clone, &map);

        let entity = &clone.children[1];
        assert_eq!(entity.targets, vec![101]);
        assert_eq!(entity.objects, vec![102]);
        assert_eq!(entity.property("Targets"), Some("[101]"));
        assert_eq!(entity.property("Objects"), Some("[102]"));
    }

    #[test]
    fn reconnect_rewrites_link_properties() {
        let template = sample_linked_group();
        let mut next_id = 100;
        let (clone, _) = duplicate_template(&template, &mut next_id);

        assert_eq!(clone.children[0].property("LinkTrId"), Some("102"));
        assert_eq!(clone.children[1].property("MisObjID"), Some("101"));
        assert_eq!(
            clone.children[1].children[0].children[0].property("TarId"),
            Some("100")
        );
    }

    #[test]
    fn reconnect_leaves_unknown_ids_untouched() {
        let src = r#"MCU_Timer
{
  Index = 5;
  Targets = [5, 999];
  Objects = [];
}"#;
        let template = parse_entity(src).unwrap().1;
        let mut next_id = 50;
        let (clone, _) = duplicate_template(&template, &mut next_id);
        assert_eq!(clone.targets, vec![50, 999]);
    }

    #[test]
    fn generate_groups_single_keeps_original_ids() {
        let template = sample_linked_group();
        let out = generate_groups(&template, 1);
        assert_eq!(out.index, Some(10));
        assert_eq!(out.children[0].index, Some(20));
    }

    #[test]
    fn generate_groups_wraps_multiple_copies() {
        let template = sample_linked_group();
        let out = generate_groups(&template, 3);
        assert_eq!(out.block_type, "Group");
        assert_eq!(out.children.len(), 3);
        assert_eq!(out.children[0].index, Some(10));
        assert_eq!(out.children[1].index, Some(22));
        assert_eq!(out.children[2].index, Some(25));
        let mut seen = HashMap::new();
        fn collect(e: &Il2Entity, seen: &mut HashMap<i32, usize>) {
            if let Some(id) = e.index {
                *seen.entry(id).or_insert(0) += 1;
            }
            for c in &e.children {
                collect(c, seen);
            }
        }
        collect(&out, &mut seen);
        assert!(seen.values().all(|&n| n == 1), "Indexes must be unique: {seen:?}");
    }

    #[test]
    fn apply_overrides_sets_country_and_aircraft() {
        let mut template = sample_linked_group();
        apply_overrides(&mut template, "mig15bis", 501);
        assert_eq!(template.children[0].property("Country"), Some("501"));
        // Vehicle is not a Plane, Script stays unset
        assert!(template.children[0].property("Script").is_none());

        let src = r#"Plane
{
  Index = 1;
  Country = 503;
  Script = "LuaScripts\WorldObjects\Planes\la11.txt";
  Model = "graphics\planes\la11\la11.mgm";
}"#;
        let mut plane = parse_entity(src).unwrap().1;
        apply_overrides(&mut plane, "mig15bis", 501);
        assert_eq!(plane.property("Country"), Some("501"));
        assert_eq!(
            plane.property("Script"),
            Some("\"LuaScripts\\WorldObjects\\Planes\\mig15bis.txt\"")
        );
        assert_eq!(
            plane.property("Model"),
            Some("\"graphics\\planes\\mig15bis\\mig15bis.mgm\"")
        );
    }
}
