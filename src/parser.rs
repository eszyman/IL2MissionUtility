//! Schema-agnostic nom parser for IL-2 .Group nested-bracket files.
//!
//! Unrecognized keys are stored as string properties. The parser never
//! requires a known schema and does not use regular expressions.

use nom::branch::alt;
use nom::bytes::complete::take_till;
use nom::character::complete::{char, digit1, multispace0, satisfy};
use nom::combinator::{map, opt, recognize};
use nom::error::{Error, ErrorKind};
use nom::multi::{many0, separated_list0};
use nom::sequence::{delimited, preceded, terminated};
use nom::{bytes::complete::take_while, combinator::map_res};
use nom::{Err, IResult, Parser};

use crate::ast::Il2Entity;

enum BodyItem {
    Property(String, String),
    Child(Il2Entity),
    ListItem(String),
}

/// Identifier: `Group`, `MCU_TR_Entity`, `XPos`, …
pub fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize((
        satisfy(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
    ))
    .parse(input)
}

/// Signed integer used by Index and MCU link arrays.
pub fn parse_integer(input: &str) -> IResult<&str, i32> {
    map_res(
        recognize((opt(char('-')), digit1)),
        |s: &str| s.parse::<i32>(),
    )
    .parse(input)
}

/// Numeric literal kept as text so serialization can round-trip formatting.
/// Accepts integers and floats (`-1`, `1000.000`, `2.730138`, `358.7`).
pub fn parse_float(input: &str) -> IResult<&str, &str> {
    recognize((opt(char('-')), digit1, opt((char('.'), digit1)))).parse(input)
}

/// Integer array: `[]`, `[43311]`, `[43590,43591]`, `[1, 2]`.
pub fn parse_integer_array(input: &str) -> IResult<&str, Vec<i32>> {
    delimited(
        (char('['), multispace0),
        separated_list0((multispace0, char(','), multispace0), parse_integer),
        (multispace0, char(']')),
    )
    .parse(input)
}

fn quoted_string_inner(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_till(|c| c == '"'), char('"')).parse(input)
}

fn quoted_list_item(input: &str) -> IResult<&str, String> {
    map(
        terminated(quoted_string_inner, (multispace0, char(';'))),
        |s: &str| format!("\"{s}\""),
    )
    .parse(input)
}

/// Unquoted vertex: `118275, 217644;` (MCU_TR_InfluenceArea Boundary).
fn coord_pair_list_item(input: &str) -> IResult<&str, String> {
    let (input, a) = parse_float(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, b) = parse_float(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, format!("{a}, {b}")))
}

fn property_value(input: &str) -> IResult<&str, String> {
    alt((
        map(quoted_string_inner, |s: &str| format!("\"{s}\"")),
        map(recognize(parse_integer_array), |s: &str| s.to_string()),
        map(take_while(|c: char| c != ';'), |s: &str| s.trim().to_string()),
    ))
    .parse(input)
}

/// Property key: `Name`, `XPos`, or a `Damaged` table index (`-1`, `0`, `3`).
fn parse_property_key(input: &str) -> IResult<&str, String> {
    alt((
        map(parse_identifier, |s: &str| s.to_string()),
        map(parse_integer, |n: i32| n.to_string()),
    ))
    .parse(input)
}

fn parse_property(input: &str) -> IResult<&str, BodyItem> {
    let (input, ident) = parse_property_key(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = property_value(input)?;
    let (input, _) = char(';')(input)?;
    Ok((input, BodyItem::Property(ident, value)))
}

fn parse_nested_block(input: &str) -> IResult<&str, BodyItem> {
    let (input, ident) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, items) = delimited(
        char('{'),
        many0(body_item),
        preceded(multispace0, char('}')),
    )
    .parse(input)?;
    Ok((
        input,
        BodyItem::Child(assemble(ident.to_string(), items)),
    ))
}

/// A property, nested block, or quoted list item. Fails without consuming
/// input when the next token is the closing `}` of the current block.
fn body_item(input: &str) -> IResult<&str, BodyItem> {
    let (after_ws, _) = multispace0(input)?;
    if after_ws.is_empty() || after_ws.starts_with('}') {
        return Err(Err::Error(Error::new(input, ErrorKind::Not)));
    }
    let (input, _) = multispace0(input)?;
    alt((
        map(quoted_list_item, BodyItem::ListItem),
        map(coord_pair_list_item, BodyItem::ListItem),
        parse_nested_block,
        parse_property,
    ))
    .parse(input)
}

fn assemble(block_type: String, items: Vec<BodyItem>) -> Il2Entity {
    let mut entity = Il2Entity::new(block_type);
    for item in items {
        match item {
            BodyItem::Property(key, value) => {
                match key.as_str() {
                    "Index" => entity.index = value.parse().ok(),
                    "Targets" => entity.targets = parse_int_array_str(&value),
                    "Objects" => entity.objects = parse_int_array_str(&value),
                    _ => {}
                }
                entity.properties.push((key, value));
            }
            BodyItem::Child(child) => entity.children.push(child),
            BodyItem::ListItem(s) => {
                entity.properties.push((String::new(), s));
            }
        }
    }
    entity
}

fn parse_int_array_str(value: &str) -> Vec<i32> {
    parse_integer_array(value)
        .map(|(_, ids)| ids)
        .unwrap_or_default()
}

/// Parse a single top-level block (`Group { … }`).
pub fn parse_entity(input: &str) -> IResult<&str, Il2Entity> {
    let (input, _) = multispace0(input)?;
    let (input, block_type) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, items) = delimited(
        char('{'),
        many0(body_item),
        preceded(multispace0, char('}')),
    )
    .parse(input)?;
    Ok((input, assemble(block_type.to_string(), items)))
}

/// Parse a complete .Group file. Leading UTF-8 BOM is ignored.
pub fn parse_group_file(input: &str) -> Result<Il2Entity, String> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    match parse_entity(input) {
        Ok((rest, entity)) => {
            let rest = rest.trim();
            if rest.is_empty() {
                Ok(entity)
            } else {
                Err(format!(
                    "trailing unparsed input ({} bytes): {}",
                    rest.len(),
                    rest.chars().take(80).collect::<String>()
                ))
            }
        }
        Err(e) => Err(format!("parse error: {e}")),
    }
}

/// Parse a .Group or editor export that may be one Group or many top-level blocks.
/// Several root blocks are wrapped in a synthetic `Airfield` group.
pub fn parse_il2_document(input: &str) -> Result<Il2Entity, String> {
    let mut rest = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut entities = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        match parse_entity(rest) {
            Ok((next, entity)) => {
                entities.push(entity);
                rest = next;
            }
            Err(e) => return Err(format!("parse error: {e}")),
        }
    }
    match entities.len() {
        0 => Err("file is empty".into()),
        1 => Ok(entities.pop().unwrap()),
        _ => {
            let mut root = Il2Entity::new("Group");
            root.set_name("Airfield");
            root.set_property("Desc", "\"\"");
            root.children = entities;
            Ok(root)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_simple() {
        assert_eq!(parse_identifier("Group"), Ok(("", "Group")));
        assert_eq!(parse_identifier("Plane"), Ok(("", "Plane")));
        assert_eq!(parse_identifier("XPos"), Ok(("", "XPos")));
    }

    #[test]
    fn identifier_mcu_types() {
        assert_eq!(parse_identifier("MCU_TR_Entity"), Ok(("", "MCU_TR_Entity")));
        assert_eq!(
            parse_identifier("MCU_CMD_AttackArea {"),
            Ok((" {", "MCU_CMD_AttackArea"))
        );
        assert_eq!(parse_identifier("OnEvents"), Ok(("", "OnEvents")));
    }

    #[test]
    fn identifier_rejects_leading_digit() {
        assert!(parse_identifier("1Group").is_err());
    }

    #[test]
    fn float_integer_form() {
        assert_eq!(parse_float("0"), Ok(("", "0")));
        assert_eq!(parse_float("-1"), Ok(("", "-1")));
        assert_eq!(parse_float("600;"), Ok((";", "600")));
    }

    #[test]
    fn float_decimal_forms() {
        assert_eq!(parse_float("147404.380"), Ok(("", "147404.380")));
        assert_eq!(parse_float("1000.000"), Ok(("", "1000.000")));
        assert_eq!(parse_float("2.730138"), Ok(("", "2.730138")));
        assert_eq!(parse_float("358.7"), Ok(("", "358.7")));
        assert_eq!(parse_float("359.9538,"), Ok((",", "359.9538")));
    }

    #[test]
    fn integer_array_empty() {
        assert_eq!(parse_integer_array("[]"), Ok(("", vec![])));
        assert_eq!(parse_integer_array("[ ]"), Ok(("", vec![])));
    }

    #[test]
    fn integer_array_single() {
        assert_eq!(parse_integer_array("[43311]"), Ok(("", vec![43311])));
    }

    #[test]
    fn integer_array_compact() {
        assert_eq!(
            parse_integer_array("[43590,43591]"),
            Ok(("", vec![43590, 43591]))
        );
    }

    #[test]
    fn integer_array_spaced() {
        assert_eq!(parse_integer_array("[1, 2]"), Ok(("", vec![1, 2])));
        assert_eq!(parse_integer_array("[0, 1, 2]"), Ok(("", vec![0, 1, 2])));
    }

    #[test]
    fn block_simple_group() {
        let src = r#"Group
{
  Name = "Truck Run";
  Index = 43322;
  Desc = "";
}"#;
        let (_, entity) = parse_entity(src).expect("parse");
        assert_eq!(entity.block_type, "Group");
        assert_eq!(entity.index, Some(43322));
        assert_eq!(entity.property("Name"), Some("\"Truck Run\""));
        assert_eq!(entity.property("Desc"), Some("\"\""));
        assert!(entity.children.is_empty());
    }

    #[test]
    fn block_mcu_with_link_arrays() {
        let src = r#"MCU_Timer
{
  Index = 43594;
  Name = "1s";
  Targets = [43593];
  Objects = [];
  Time = 1;
}"#;
        let (_, entity) = parse_entity(src).expect("parse");
        assert_eq!(entity.block_type, "MCU_Timer");
        assert_eq!(entity.index, Some(43594));
        assert_eq!(entity.targets, vec![43593]);
        assert_eq!(entity.objects, vec![]);
        assert_eq!(entity.property("Time"), Some("1"));
    }

    #[test]
    fn property_keeps_clock_time_and_dotted_date() {
        let src = r#"MCU_DateTime
{
  Index = 1;
  Time = 13:0:0;
  Date = 1.6.1951;
}"#;
        let (_, entity) = parse_entity(src).expect("parse");
        assert_eq!(entity.property("Time"), Some("13:0:0"));
        assert_eq!(entity.property("Date"), Some("1.6.1951"));
    }

    #[test]
    fn block_nested_children() {
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
    Targets = [];
    Objects = [];
    MisObjID = 2;
  }
}"#;
        let (_, entity) = parse_entity(src).expect("parse");
        assert_eq!(entity.children.len(), 2);
        assert_eq!(entity.children[0].block_type, "Vehicle");
        assert_eq!(entity.children[0].index, Some(2));
        assert_eq!(entity.children[1].block_type, "MCU_TR_Entity");
        assert_eq!(entity.children[1].index, Some(3));
        assert_eq!(entity.max_index(), 3);
    }

    #[test]
    fn damaged_table_numeric_keys() {
        let src = r#"Block
{
  Name = "Block";
  Damaged
  {
    -1 = 1;
    0 = 1;
    2 = 1;
  }
}"#;
        let (_, entity) = parse_entity(src).expect("Damaged numeric keys");
        assert_eq!(entity.children.len(), 1);
        let damaged = &entity.children[0];
        assert_eq!(damaged.block_type, "Damaged");
        assert_eq!(damaged.property("-1"), Some("1"));
        assert_eq!(damaged.property("0"), Some("1"));
        assert_eq!(damaged.property("2"), Some("1"));
    }

    #[test]
    fn block_unrecognized_keys_are_kept() {
        let src = r#"FooBar
{
  MysteryKey = "abc";
  AlsoUnknown = 42;
  NestedUnknown
  {
    Qux = 1.5;
  }
}"#;
        let (_, entity) = parse_entity(src).expect("unrecognized keys must not fail");
        assert_eq!(entity.block_type, "FooBar");
        assert_eq!(entity.property("MysteryKey"), Some("\"abc\""));
        assert_eq!(entity.property("AlsoUnknown"), Some("42"));
        assert_eq!(entity.children.len(), 1);
        assert_eq!(entity.children[0].block_type, "NestedUnknown");
        assert_eq!(entity.children[0].property("Qux"), Some("1.5"));
    }

    #[test]
    fn block_quoted_list_items() {
        let src = r#"Trailers
{
  "luascripts\worldobjects\vehicles\trailers\zpuvz53-trailer.txt";
}"#;
        let (_, entity) = parse_entity(src).expect("parse trailers");
        assert_eq!(entity.block_type, "Trailers");
        assert_eq!(
            entity.properties,
            vec![(
                String::new(),
                "\"luascripts\\worldobjects\\vehicles\\trailers\\zpuvz53-trailer.txt\"".to_string()
            )]
        );
    }

    #[test]
    fn influence_area_boundary_coord_pairs() {
        let src = r#"MCU_TR_InfluenceArea
{
  Index = 1;
  Name = "USA Influence Area";
  Country = 601;
  Boundary
  {
    118275, 217644;
    133427, 218780;
  }
}"#;
        let (_, entity) = parse_entity(src).expect("parse influence area");
        assert_eq!(entity.block_type, "MCU_TR_InfluenceArea");
        assert_eq!(entity.name(), Some("USA Influence Area"));
        assert_eq!(entity.property("Country"), Some("601"));
        assert_eq!(entity.children.len(), 1);
        assert_eq!(entity.children[0].block_type, "Boundary");
        assert_eq!(
            entity.children[0].properties,
            vec![
                (String::new(), "118275, 217644".to_string()),
                (String::new(), "133427, 218780".to_string()),
            ]
        );
    }

    #[test]
    fn block_on_events_and_reports() {
        let src = r#"MCU_TR_Entity
{
  Index = 12;
  Targets = [];
  Objects = [];
  MisObjID = 23;
  OnEvents
  {
    OnEvent
    {
      Type = 4;
      TarId = 25;
    }
  }
  OnReports
  {
    OnReport
    {
      Type = 0;
      CmdId = 26;
      TarId = 27;
    }
  }
}"#;
        let (_, entity) = parse_entity(src).expect("parse events");
        assert_eq!(entity.children.len(), 2);
        let event = &entity.children[0].children[0];
        assert_eq!(event.block_type, "OnEvent");
        assert_eq!(event.property("TarId"), Some("25"));
        let report = &entity.children[1].children[0];
        assert_eq!(report.property("CmdId"), Some("26"));
    }

    #[test]
    fn parse_group_file_rejects_trailing_garbage() {
        let err = parse_group_file("Group { Index = 1; } leftover").unwrap_err();
        assert!(err.contains("trailing"));
    }

    #[test]
    fn parse_il2_document_wraps_root_level_blocks() {
        let src = "MCU_Timer\n{\n  Index = 1;\n  Name = \"A\";\n}\n\nMCU_Timer\n{\n  Index = 2;\n  Name = \"B\";\n}\n";
        let root = parse_il2_document(src).expect("forest");
        assert_eq!(root.block_type, "Group");
        assert_eq!(root.name(), Some("Airfield"));
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name(), Some("A"));
        assert_eq!(root.children[1].name(), Some("B"));
        let one = parse_il2_document("Group { Index = 9; Name = \"Solo\"; }").unwrap();
        assert_eq!(one.name(), Some("Solo"));
        assert_eq!(one.index, Some(9));
    }

    #[test]
    fn parse_real_truck_template() {
        let src = include_str!("../TemplateExamples/GroundUnits/DropIns/DPRK Truck Run.Group");
        let entity = parse_group_file(src).expect("parse truck template");
        assert_eq!(entity.block_type, "Group");
        assert!(entity.index.is_some());
        assert!(
            count_block_type(&entity, "Vehicle") > 0,
            "expected nested Vehicle entities"
        );
    }

    #[test]
    fn parse_real_artillery_template() {
        let src = include_str!("../TemplateExamples/GroundUnits/DropIns/DPRK MM13 Rocket Arty.Group");
        let entity = parse_group_file(src).expect("parse artillery template");
        assert_eq!(entity.block_type, "Group");
        assert!(entity.index.is_some());
    }

    #[test]
    fn parse_real_fighters_template() {
        let src = include_str!("../TemplateExamples/Eastern_Fighters_Random_5pack_V6.Group");
        let entity = parse_group_file(src).expect("parse fighters template");
        assert_eq!(entity.block_type, "Group");
        assert_eq!(entity.index, Some(3));
        let plane_count = count_block_type(&entity, "Plane");
        assert!(plane_count > 0, "expected Plane entities");
    }

    #[test]
    fn parse_improved_cooldown_logic() {
        let src = include_str!("../TemplateExamples/ImprovedCooldownLogic.Group");
        let entity = parse_group_file(src).expect("parse ImprovedCooldownLogic");
        assert_eq!(entity.block_type, "Group");
        assert_eq!(count_block_type(&entity, "MCU_ModifierSetVal"), 1);
        assert_eq!(count_block_type(&entity, "MCU_Spawner"), 1);
        let modifier = entity.find_by_name("Modifier Set Value").unwrap();
        assert_eq!(modifier.property("ParamIndex"), Some("0"));
        assert_eq!(modifier.property("Data0"), Some("0"));
        let death = entity.find_by_name("DeathCount").unwrap();
        assert_eq!(modifier.targets, vec![death.index.unwrap()]);
    }

    fn count_block_type(entity: &Il2Entity, block_type: &str) -> usize {
        let mut n = usize::from(entity.block_type == block_type);
        for child in &entity.children {
            n += count_block_type(child, block_type);
        }
        n
    }
}
