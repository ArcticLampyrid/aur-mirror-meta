use itertools::Itertools;
use std::collections::{hash_map, HashMap};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSrcInfo {
    pub pkgbase: String,
    pub pkgname: String,
    pub properties: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct PkgBase {
    pkgbase: String,
    properties: HashMap<String, Vec<String>>,
}

fn merge_props(dst: &mut HashMap<String, Vec<String>>, src: &HashMap<String, Vec<String>>) {
    for (k, v) in src {
        let entry = dst.entry(k.clone());
        if let hash_map::Entry::Vacant(vacant) = entry {
            vacant.insert(v.clone());
        }
    }
}

impl ParsedSrcInfo {
    pub fn parse(srcinfo_text: &str) -> Vec<ParsedSrcInfo> {
        if srcinfo_text.trim().is_empty() {
            return vec![];
        }

        let lines = srcinfo_text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty());

        let mut result: Vec<ParsedSrcInfo> = Vec::new();
        let mut current_base: Option<PkgBase> = None;
        let mut current_pkg: Option<ParsedSrcInfo> = None;

        for line in lines {
            if let Some(eq_pos) = line.find('=') {
                let (key, value) = line.split_at(eq_pos);
                let trimmed_key = key.trim();
                let trimmed_value = value[1..].trim();

                match trimmed_key {
                    "pkgbase" => {
                        if let Some(mut pkg) = current_pkg.take() {
                            if let Some(base) = &current_base {
                                merge_props(&mut pkg.properties, &base.properties);
                            }
                            result.push(pkg);
                        }
                        current_base = Some(PkgBase {
                            pkgbase: trimmed_value.to_string(),
                            properties: HashMap::new(),
                        });
                    }
                    "pkgname" => {
                        if let Some(base) = &current_base {
                            if let Some(mut pkg) = current_pkg.take() {
                                merge_props(&mut pkg.properties, &base.properties);
                                result.push(pkg);
                            }

                            current_pkg = Some(ParsedSrcInfo {
                                pkgbase: base.pkgbase.clone(),
                                pkgname: trimmed_value.to_string(),
                                properties: HashMap::new(),
                            });
                        }
                    }
                    other_key => {
                        if let Some(pkg) = current_pkg.as_mut() {
                            let vec = pkg.properties.entry(other_key.to_string()).or_default();
                            if !trimmed_value.is_empty() {
                                vec.push(trimmed_value.to_string());
                            }
                        } else if let Some(base) = current_base.as_mut() {
                            let vec = base.properties.entry(other_key.to_string()).or_default();
                            if !trimmed_value.is_empty() {
                                vec.push(trimmed_value.to_string());
                            }
                        }
                    }
                }
            }
        }

        if let Some(mut pkg) = current_pkg.take() {
            if let Some(base) = &current_base {
                merge_props(&mut pkg.properties, &base.properties);
            }
            result.push(pkg);
        }

        if result.is_empty() {
            if let Some(base) = current_base.take() {
                warn!(".SRCINFO without pkgname found, using pkgbase as pkgname");
                result.push(ParsedSrcInfo {
                    pkgname: base.pkgbase.clone(),
                    pkgbase: base.pkgbase,
                    properties: base.properties,
                });
            }
        }

        result
    }

    pub fn first_prop(&self, k: &str) -> Option<&str> {
        self.properties
            .get(k)
            .and_then(|v| v.first().map(|s| s.as_str()))
    }

    pub fn prop(&self, k: &str) -> Vec<String> {
        self.properties.get(k).cloned().unwrap_or_default()
    }

    pub fn flatten_arch_prop(&self, k: &str) -> Vec<String> {
        // join all key named ${k} or starts with ${k}_
        // dedup and flatten

        let prefix = &format!("{}_", k);
        self.properties
            .iter()
            .filter(|(key, _)| *key == k || key.starts_with(prefix))
            .flat_map(|(_, values)| values)
            .dedup()
            .cloned()
            .collect()
    }

    pub fn version(&self) -> String {
        let epoch = self.first_prop("epoch");
        let pkgver = self.first_prop("pkgver").unwrap_or("0.0.1");
        let pkgrel = self.first_prop("pkgrel").unwrap_or("1");
        if let Some(epoch) = epoch {
            format!("{}:{}-{}", epoch, pkgver, pkgrel)
        } else {
            format!("{}-{}", pkgver, pkgrel)
        }
    }
}
