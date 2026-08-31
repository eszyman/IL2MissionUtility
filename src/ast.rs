/// Schema-agnostic IL-2 .Group AST node.
///
/// Unrecognized keys are stored as string properties and never cause a parse
/// failure. MCU link arrays (`Targets`, `Objects`) and `Index` are lifted into
/// dedicated fields so the duplication engine can reallocate and reconnect them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Il2Entity {
    pub block_type: String,
    pub index: Option<i32>,
    pub targets: Vec<i32>,
    pub objects: Vec<i32>,
    pub properties: Vec<(String, String)>,
    pub children: Vec<Il2Entity>,
}

impl Il2Entity {
    pub fn new(block_type: impl Into<String>) -> Self {
        Self {
            block_type: block_type.into(),
            index: None,
            targets: Vec::new(),
            objects: Vec::new(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_property(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some((_, existing)) = self.properties.iter_mut().find(|(k, _)| k == key) {
            *existing = value;
        } else {
            self.properties.push((key.to_string(), value));
        }
    }

    /// Update `key` only when the prototype already has it. Ground vehicles
    /// must not gain plane-only keys such as `AiRTBDecision` / `StartType`.
    pub fn set_existing_property(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some((_, existing)) = self.properties.iter_mut().find(|(k, _)| k == key) {
            *existing = value;
        }
    }

    /// Highest `Index` in this subtree (0 if none).
    pub fn max_index(&self) -> i32 {
        let own = self.index.unwrap_or(0);
        self.children
            .iter()
            .map(Self::max_index)
            .max()
            .unwrap_or(0)
            .max(own)
    }

    /// `Name` property with surrounding quotes stripped.
    pub fn name(&self) -> Option<&str> {
        self.property("Name").map(|v| v.trim_matches('"'))
    }

    pub fn set_name(&mut self, name: &str) {
        self.set_property("Name", format!("\"{name}\""));
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Il2Entity> {
        if self.name() == Some(name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_by_name(name))
    }

    pub fn find_by_name_mut(&mut self, name: &str) -> Option<&mut Il2Entity> {
        if self.name() == Some(name) {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|c| c.find_by_name_mut(name))
    }

    #[allow(dead_code)]
    pub fn find_all_by_name<'a>(&'a self, name: &str, out: &mut Vec<&'a Il2Entity>) {
        if self.name() == Some(name) {
            out.push(self);
        }
        for child in &self.children {
            child.find_all_by_name(name, out);
        }
    }

    #[allow(dead_code)]
    pub fn count_block_type(&self, block_type: &str) -> usize {
        let mut n = usize::from(self.block_type == block_type);
        for child in &self.children {
            n += child.count_block_type(block_type);
        }
        n
    }

    #[allow(dead_code)]
    pub fn collect_indexes(&self, out: &mut Vec<i32>) {
        if let Some(id) = self.index {
            out.push(id);
        }
        for child in &self.children {
            child.collect_indexes(out);
        }
    }

    pub fn set_targets(&mut self, ids: Vec<i32>) {
        self.targets = ids;
        self.set_property("Targets", format_int_array(&self.targets));
    }

    pub fn set_objects(&mut self, ids: Vec<i32>) {
        self.objects = ids;
        self.set_property("Objects", format_int_array(&self.objects));
    }

    pub fn for_each_mut<F: FnMut(&mut Il2Entity)>(&mut self, f: &mut F) {
        f(self);
        for child in &mut self.children {
            child.for_each_mut(f);
        }
    }

    pub fn for_each<F: FnMut(&Il2Entity)>(&self, f: &mut F) {
        f(self);
        for child in &self.children {
            child.for_each(f);
        }
    }

    pub fn append_target(&mut self, id: i32) {
        if self.targets.contains(&id) {
            return;
        }
        let mut ids = self.targets.clone();
        ids.push(id);
        self.set_targets(ids);
    }

    pub fn set_ypos(&mut self, y: f64) {
        let decimals = self
            .property("YPos")
            .and_then(|v| v.split('.').nth(1))
            .map(|s| s.len())
            .unwrap_or(3);
        self.set_property("YPos", format!("{y:.decimals$}"));
    }

    pub fn replace_target_id(&mut self, old: i32, new: i32) {
        if old == new {
            return;
        }
        let mut changed = false;
        for id in &mut self.targets {
            if *id == old {
                *id = new;
                changed = true;
            }
        }
        if changed {
            self.set_property("Targets", format_int_array(&self.targets));
        }
        for child in &mut self.children {
            child.replace_target_id(old, new);
        }
    }

    pub fn translate_xz(&mut self, dx: f64, dz: f64) {
        bump_pos(&mut self.properties, "XPos", dx);
        bump_pos(&mut self.properties, "ZPos", dz);
        for child in &mut self.children {
            child.translate_xz(dx, dz);
        }
    }

    pub fn pos_xz(&self) -> Option<(f64, f64)> {
        let x = self.property("XPos")?.parse().ok()?;
        let z = self.property("ZPos")?.parse().ok()?;
        Some((x, z))
    }

    /// First X/Z on this node or a descendant. Group wrappers often have none.
    pub fn first_xz(&self) -> Option<(f64, f64)> {
        self.pos_xz()
            .or_else(|| self.children.iter().find_map(Il2Entity::first_xz))
    }
}

pub fn format_int_array(ids: &[i32]) -> String {
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

fn bump_pos(properties: &mut [(String, String)], key: &str, delta: f64) {
    if delta == 0.0 {
        return;
    }
    let Some((_, value)) = properties.iter_mut().find(|(k, _)| k == key) else {
        return;
    };
    let Ok(n) = value.parse::<f64>() else {
        return;
    };
    let decimals = value.split('.').nth(1).map(|s| s.len()).unwrap_or(0);
    *value = if decimals == 0 {
        format!("{:.0}", n + delta)
    } else {
        format!("{:.decimals$}", n + delta)
    };
}
