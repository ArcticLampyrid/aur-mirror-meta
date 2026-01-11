use serde::{Deserialize, Serialize};

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
    pub first_submitted: i64,
    #[serde(rename = "LastModified")]
    pub last_modified: i64,
    #[serde(rename = "OutOfDate")]
    pub out_of_date: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcPackageDetails {
    #[serde(rename = "ID")]
    pub id: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
    #[serde(rename = "PackageBase")]
    pub package_base: String,
    #[serde(rename = "PackageBaseID")]
    pub package_base_id: u32,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "URL")]
    pub url: Option<String>,
    #[serde(rename = "URLPath")]
    pub url_path: String,
    #[serde(rename = "Maintainer", default)]
    pub maintainer: Option<String>,
    #[serde(rename = "Submitter", default)]
    pub submitter: Option<String>,
    #[serde(rename = "NumVotes")]
    pub num_votes: u32,
    #[serde(rename = "Popularity")]
    pub popularity: f64,
    #[serde(rename = "FirstSubmitted")]
    pub first_submitted: i64,
    #[serde(rename = "LastModified")]
    pub last_modified: i64,
    #[serde(rename = "OutOfDate", default)]
    pub out_of_date: Option<i64>,
    #[serde(rename = "License", default)]
    pub license: Vec<String>,
    #[serde(rename = "Depends", default)]
    pub depends: Vec<String>,
    #[serde(rename = "MakeDepends", default)]
    pub makedepends: Vec<String>,
    #[serde(rename = "OptDepends", default)]
    pub optdepends: Vec<String>,
    #[serde(rename = "CheckDepends", default)]
    pub checkdepends: Vec<String>,
    #[serde(rename = "Provides", default)]
    pub provides: Vec<String>,
    #[serde(rename = "Conflicts", default)]
    pub conflicts: Vec<String>,
    #[serde(rename = "Replaces", default)]
    pub replaces: Vec<String>,
    #[serde(rename = "Groups", default)]
    pub groups: Vec<String>,
    #[serde(rename = "Keywords", default)]
    pub keywords: Vec<String>,
    #[serde(rename = "CoMaintainers", default)]
    pub co_maintainers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DatabasePackageInfoWithSupplement {
    pub branch: String,
    #[allow(unused)]
    pub commit_id: String,
    #[allow(unused)]
    pub committed_at: i64,
    pub pkg_name: String,
    pub pkg_desc: Option<String>,
    pub version: String,
    pub url: Option<String>,
    // Supplemented metadata
    pub popularity: Option<f64>,
    pub num_votes: Option<i64>,
    pub out_of_date: Option<i64>,
    pub maintainer: Option<String>,
    pub submitter: Option<String>,
    pub first_submitted: Option<i64>,
    pub last_modified: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DatabasePackageDetailsWithSupplement {
    pub info: DatabasePackageInfoWithSupplement,
    pub depends: Vec<String>,
    pub make_depends: Vec<String>,
    pub opt_depends: Vec<String>,
    pub check_depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub groups: Vec<String>,
    // Supplemented metadata
    pub keywords: Vec<String>,
    pub co_maintainers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DatabasePackageDetails {
    pub branch: String,
    pub commit_id: String,
    pub committed_at: i64,
    pub pkg_name: String,
    pub pkg_desc: Option<String>,
    pub version: String,
    pub url: Option<String>,
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

#[derive(Debug, Clone)]
pub struct DatabaseSupplementData {
    pub pkgname: String,
    pub version: String,
    pub popularity: f64,
    pub num_votes: u32,
    pub out_of_date: Option<i64>,
    pub maintainer: Option<String>,
    pub submitter: Option<String>,
    pub co_maintainers: Vec<String>,
    pub keywords: Vec<String>,
    pub first_submitted: i64,
    pub last_modified: i64,
}
