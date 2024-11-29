use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub scripts: HashMap<String, String>,
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "optionalDependencies")]
    pub optional_dependencies: Option<HashMap<String, String>>,
    pub bin: Option<Bin>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Bin {
    Single(String),
    Multiple(HashMap<String, String>),
}
