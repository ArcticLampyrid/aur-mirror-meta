use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RpcResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "resultcount")]
    pub result_count: usize,
    pub results: Vec<T>,
    #[serde(rename = "type")]
    pub response_type: String,
    pub version: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct RpcPackageInfo {
    #[serde(rename = "ID")]
    pub id: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "PackageBase")]
    pub package_base: String,
    #[serde(rename = "PackageBaseID")]
    pub package_base_id: u32,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "URLPath")]
    pub url_path: String,
    #[serde(rename = "Maintainer")]
    pub maintainer: String,
    #[serde(rename = "NumVotes")]
    pub num_votes: u32,
    #[serde(rename = "Popularity")]
    pub popularity: f64,
    #[serde(rename = "FirstSubmitted")]
    pub first_submitted: u64,
    #[serde(rename = "LastModified")]
    pub last_modified: u64,
    #[serde(rename = "OutOfDate")]
    pub out_of_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RpcPackageDetails {
    #[serde(rename = "ID")]
    pub id: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "PackageBase")]
    pub package_base: String,
    #[serde(rename = "PackageBaseID")]
    pub package_base_id: u32,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "URLPath")]
    pub url_path: String,
    #[serde(rename = "Maintainer")]
    pub maintainer: String,
    #[serde(rename = "Submitter")]
    pub submitter: String,
    #[serde(rename = "NumVotes")]
    pub num_votes: u32,
    #[serde(rename = "Popularity")]
    pub popularity: f64,
    #[serde(rename = "FirstSubmitted")]
    pub first_submitted: u64,
    #[serde(rename = "LastModified")]
    pub last_modified: u64,
    #[serde(rename = "OutOfDate")]
    pub out_of_date: Option<String>,
    #[serde(rename = "License")]
    pub license: Vec<String>,
    #[serde(rename = "Depends")]
    pub depends: Vec<String>,
    #[serde(rename = "MakeDepends")]
    pub makedepends: Vec<String>,
    #[serde(rename = "OptDepends")]
    pub optdepends: Vec<String>,
    #[serde(rename = "CheckDepends")]
    pub checkdepends: Vec<String>,
    #[serde(rename = "Provides")]
    pub provides: Vec<String>,
    #[serde(rename = "Conflicts")]
    pub conflicts: Vec<String>,
    #[serde(rename = "Replaces")]
    pub replaces: Vec<String>,
    #[serde(rename = "Groups")]
    pub groups: Vec<String>,
    #[serde(rename = "Keywords")]
    pub keywords: Vec<String>,
    #[serde(rename = "CoMaintainers")]
    pub co_maintainers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DatabasePackageInfo {
    pub branch: String,
    pub commit_id: String,
    pub pkg_name: String,
    pub pkg_desc: Option<String>,
    pub version: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DatabasePackageDetails {
    pub info: DatabasePackageInfo,
    pub depends: Vec<String>,
    pub make_depends: Vec<String>,
    pub opt_depends: Vec<String>,
    pub check_depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchType {
    Name,
    NameDesc,
    Depends,
    MakeDepends,
    OptDepends,
    CheckDepends,
}

impl SearchType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "name" => Some(Self::Name),
            "name-desc" => Some(Self::NameDesc),
            "depends" => Some(Self::Depends),
            "makedepends" => Some(Self::MakeDepends),
            "optdepends" => Some(Self::OptDepends),
            "checkdepends" => Some(Self::CheckDepends),
            _ => None,
        }
    }
}
