//! Serialize an `Il2Entity` AST back to IL-2 .Group text.

use crate::ast::Il2Entity;

const NEWLINE: &str = "\r\n";

/// Write `entity` using IL-2 nested-bracket formatting (CRLF, 2-space indent).
pub fn serialize_group(entity: &Il2Entity) -> String {
    let mut out = String::new();
    write_entity(&mut out, entity, 0);
    out
}

fn write_entity(out: &mut String, entity: &Il2Entity, indent: usize) {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);

    out.push_str(&pad);
    out.push_str(&entity.block_type);
    out.push_str(NEWLINE);
    out.push_str(&pad);
    out.push('{');
    out.push_str(NEWLINE);

    for (key, value) in &entity.properties {
        if key.is_empty() {
            out.push_str(&inner);
            out.push_str(value);
            out.push(';');
            out.push_str(NEWLINE);
            continue;
        }
        let rendered = match key.as_str() {
            "Index" => entity
                .index
                .map(|id| id.to_string())
                .unwrap_or_else(|| value.clone()),
            "Targets" => format_int_array(&entity.targets),
            "Objects" => format_int_array(&entity.objects),
            _ => value.clone(),
        };
        out.push_str(&inner);
        out.push_str(key);
        out.push_str(" = ");
        out.push_str(&rendered);
        out.push(';');
        out.push_str(NEWLINE);
    }

    for child in &entity.children {
        write_entity(out, child, indent + 1);
        out.push_str(NEWLINE);
        out.push_str(&inner);
        out.push_str(NEWLINE);
    }

    out.push_str(&pad);
    out.push('}');
    out.push_str(NEWLINE);
}

fn format_int_array(ids: &[i32]) -> String {
    if ids.is_empty() {
        "[]".to_string()
    } else {
        let inner = ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("[{inner}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate::{apply_overrides, duplicate_template, generate_groups};
    use crate::parser::{parse_entity, parse_group_file};

    #[test]
    fn serialize_simple_group_formatting() {
        let src = "Group\r\n{\r\n  Name = \"Truck Run\";\r\n  Index = 43322;\r\n  Desc = \"\";\r\n}\r\n";
        let entity = parse_group_file(src).expect("parse");
        let out = serialize_group(&entity);
        assert_eq!(out, src);
    }

    #[test]
    fn serialize_mcu_link_arrays() {
        let src = r#"MCU_Timer
{
  Index = 43594;
  Name = "1s";
  Targets = [43590,43591];
  Objects = [];
  Time = 1;
}"#;
        let entity = parse_entity(src).unwrap().1;
        let out = serialize_group(&entity);
        assert!(out.contains("Targets = [43590,43591];"));
        assert!(out.contains("Objects = [];"));
        assert!(out.contains("Index = 43594;"));
        assert!(out.contains("\r\n"));
    }

    #[test]
    fn serialize_quoted_list_item() {
        let src = "Trailers\r\n{\r\n  \"luascripts\\worldobjects\\vehicles\\trailers\\zpuvz53-trailer.txt\";\r\n}\r\n";
        let entity = parse_group_file(src).expect("parse");
        let out = serialize_group(&entity);
        assert_eq!(out, src);
    }

    #[test]
    fn round_trip_nested_ast() {
        let src = r#"Group
{
  Name = "Root";
  Index = 1;
  Vehicle
  {
    Name = "car";
    Index = 2;
    LinkTrId = 3;
  }
  MCU_TR_Entity
  {
    Index = 3;
    Targets = [2];
    Objects = [];
    MisObjID = 2;
  }
}"#;
        let original = parse_entity(src).unwrap().1;
        let text = serialize_group(&original);
        let reparsed = parse_group_file(&text).expect("reparse serialized");
        assert_eq!(original, reparsed);
    }

    #[test]
    fn round_trip_real_truck_template() {
        let src = include_str!("../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group");
        let original = parse_group_file(src).expect("parse truck");
        let text = serialize_group(&original);
        let reparsed = parse_group_file(&text).expect("reparse truck");
        assert_eq!(original.block_type, reparsed.block_type);
        assert_eq!(original.index, reparsed.index);
        assert_eq!(original.max_index(), reparsed.max_index());
        assert_eq!(count_nodes(&original), count_nodes(&reparsed));
    }

    #[test]
    fn serialize_after_duplication_uses_new_ids() {
        let src = r#"MCU_Timer
{
  Index = 5;
  Targets = [5];
  Objects = [];
}"#;
        let template = parse_entity(src).unwrap().1;
        let mut next_id = 80;
        let (clone, _) = duplicate_template(&template, &mut next_id);
        let out = serialize_group(&clone);
        assert!(out.contains("Index = 80;"));
        assert!(out.contains("Targets = [80];"));
        assert!(!out.contains("Index = 5;"));
    }

    #[test]
    fn full_pipeline_generate_and_serialize() {
        let src = include_str!("../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group");
        let template = parse_group_file(src).expect("parse truck");
        let mut generated = generate_groups(&template, 2);
        apply_overrides(&mut generated, "", 501);
        let text = serialize_group(&generated);
        let reparsed = parse_group_file(&text).expect("reparse generated");
        assert_eq!(reparsed.children.len(), 2);
        assert_eq!(reparsed.children[0].index, template.index);
        assert_ne!(reparsed.children[1].index, template.index);
        let countries = collect_property(&reparsed, "Country");
        assert!(!countries.is_empty());
        assert!(countries.iter().all(|c| c == "501"));
    }

    fn count_nodes(entity: &Il2Entity) -> usize {
        1 + entity.children.iter().map(count_nodes).sum::<usize>()
    }

    fn collect_property(entity: &Il2Entity, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(v) = entity.property(key) {
            out.push(v.to_string());
        }
        for child in &entity.children {
            out.extend(collect_property(child, key));
        }
        out
    }
}
