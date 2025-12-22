# AUR Mirror Metadata System - Product Requirements Document

## Project Overview
AUR Mirror Meta (AMM) is a system that builds on AUR GitHub Mirror and provides compatible endpoints for AUR helpers to interact with. It mirrors and indexes metadata from the Arch User Repository (AUR) to provide a fast, searchable interface compatible with the AUR RPC API.

## Feature 1: AUR Metadata Fetching

### 1.1 Branch Discovery
**Requirement**: Fetch all available branches from the AUR Git repository
- **Endpoint**: `https://github.com/archlinux/aur.git/info/refs?service=git-upload-pack`
- **Authentication**: GitHub token (optional)
- **Output**: List of branch references (`refs/heads/*`) with corresponding commit IDs, excluding `main` branch
- **Data Structure**: `HashMap<String, String>` (branch name → commit ID)

### 1.2 SRCINFO Content Retrieval
**Requirement**: Retrieve `.SRCINFO` files for each branch using Git Http(s) Protocol V2
- **Authentication**: GitHub token (optional)
- **Batch Size**: 3000 commits per query
- **Fetch Logic**:
  1. Do a blobless (`filter blob:none`) fetch to get commit & tree objects in packfile response
  2. Parse commit & tree objects to locate `.SRCINFO` blobs (only get IDs here)
  3. Do a second fetch to retrieve only the `.SRCINFO` blobs using their IDs

## Feature 2: SRCINFO Parsing and Indexing

### 2.1 SRCINFO Parser
**Requirement**: Parse `.SRCINFO` files according to PKGBUILD format specification

**Format Specification**:
- **Structure**: Key-value pairs with tab-indented continuation
- **Hierarchy**: `pkgbase` defines base package, `pkgname` defines individual packages
- **Inheritance**: Each `pkgname` inherits all `pkgbase` attributes by default
- **Override**: Package-level attributes override base attributes

**Sample SRCINFO Format**:
```
pkgbase = package-base-name
    pkgdesc = Package description
    pkgver = 1.0.0
    pkgrel = 1
    url = https://example.com
    license = MIT
    depends = dependency1
    depends = dependency2

pkgname = package-name
    depends = override-dependency
    conflicts = conflicting-package
```

**Parsing Rules**:
1. Each `pkgbase` section starts a new package base
2. Each `pkgname` section starts a new package within the base
3. Package attributes override base attributes (replacement for array, not extending)
4. Multi-value fields (depends, makedepends, etc.) are collected as arrays

### 2.2 Database Indexing
**Requirement**: Extract and index parsed package information for fast search

**Database Schema**:
| Table Name        | Fields                                              | Primary Key                      |
| ----------------- | --------------------------------------------------- | -------------------------------- |
| branch_commits    | branch, commit_id                                   | branch                           |
| pkg_info          | branch, pkg_name, pkg_desc, version, url, commit_id | (branch, pkg_name)               |
| pkg_depends       | branch, pkg_name, depend                            | (branch, pkg_name, depend)       |
| pkg_make_depends  | branch, pkg_name, make_depend                       | (branch, pkg_name, make_depend)  |
| pkg_opt_depends   | branch, pkg_name, opt_depend                        | (branch, pkg_name, opt_depend)   |
| pkg_check_depends | branch, pkg_name, check_depend                      | (branch, pkg_name, check_depend) |
| pkg_provides      | branch, pkg_name, provide                           | (branch, pkg_name, provide)      |
| pkg_conflicts     | branch, pkg_name, conflict                          | (branch, pkg_name, conflict)     |
| pkg_replaces      | branch, pkg_name, replace                           | (branch, pkg_name, replace)      |
| pkg_groups        | branch, pkg_name, group_name                        | (branch, pkg_name, group_name)   |

**Required Indexes**:
| Index Name                         | Table Name        | Keys         |
| ---------------------------------- | ----------------- | ------------ |
| idx_pkg_info_name                  | pkg_info          | pkg_name     |
| idx_pkg_info_branch                | pkg_info          | branch       |
| idx_pkg_depends_branch             | pkg_depends       | branch       |
| idx_pkg_make_depends_branch        | pkg_make_depends  | branch       |
| idx_pkg_opt_depends_branch         | pkg_opt_depends   | branch       |
| idx_pkg_check_depends_branch       | pkg_check_depends | branch       |
| idx_pkg_provides_branch            | pkg_provides      | branch       |
| idx_pkg_conflicts_branch           | pkg_conflicts     | branch       |
| idx_pkg_replaces_branch            | pkg_replaces      | branch       |
| idx_pkg_groups_branch              | pkg_groups        | branch       |
| idx_pkg_depends_depend             | pkg_depends       | depend       |
| idx_pkg_make_depends_make_depend   | pkg_make_depends  | make_depend  |
| idx_pkg_opt_depends_opt_depend     | pkg_opt_depends   | opt_depend   |
| idx_pkg_check_depends_check_depend | pkg_check_depends | check_depend |

### 2.3 Incremental Update Strategy
**Requirement**: Efficiently update package indexes when source data changes

**Update Logic**:
1. **Commit Comparison**: Skip indexing if `commit_id` unchanged for branch
2. **Transactional Updates**: Use database transactions to ensure consistency across all tables
3. **Branch Cleanup**: For changed branches:
   - Delete all existing data for the branch from all tables
   - Insert new parsed package data
   - Update branch commit tracking
   - Commit transaction atomically
4. **Batch Processing**: Process multiple branches in single transactions for efficiency

## Feature 3: AUR RPC API Implementation

### 3.1 API Overview
**Requirement**: Implement AUR-compatible RPC interface for package search and information retrieval
- **Protocol**: HTTP REST API
- **Supported Version**: v5 only
- **Content-Type**: `application/json`
- **Methods**: GET, POST

### 3.2 Search API
**Endpoint**: `/rpc`

**Parameters**:
- `v=5` (required): API version
- `type=search` (required): Request type
- `by=<field>` (optional): Search field, defaults to `name-desc`
- `arg=<keywords>` (required): Search keywords
- `callback=<function>` (optional): JSONP callback function

**Supported Search Fields**:
- `name`: Search package names only (LIKE pattern match)
- `name-desc`: Search package names and descriptions (default, LIKE pattern match)
- `depends`: Find packages that depend on the keyword (exact match)
- `makedepends`: Find packages with build dependency on keyword (exact match)
- `optdepends`: Find packages with optional dependency on keyword (exact match)
- `checkdepends`: Find packages with check dependency on keyword (exact match)

**Search Logic**:
- Name/description searches use SQL LIKE with wildcard patterns (%keyword%)
- Dependency searches use exact string matching
- Returns distinct results to avoid duplicates

**Examples**:
```
GET /rpc?v=5&type=search&arg=firefox
GET /rpc?v=5&type=search&by=name&arg=firefox
GET /rpc?v=5&type=search&by=makedepends&arg=boost
GET /rpc?v=5&type=search&arg=editor&callback=myCallback
```

### 3.3 Package Info API  
**Endpoint**: `/rpc`

**Parameters**:
- `v=5` (required): API version
- `type=info` (required): Request type
- `arg[]=<pkg>` or `arg=<pkg>`: Package name(s) to query

**Parameter Handling**:
- **Batch Queries**: Multiple packages can be queried in single request

**Examples**:
```
GET /rpc?v=5&type=info&arg[]=firefox
GET /rpc?v=5&type=info&arg[]=firefox&arg[]=chromium
POST /rpc (with form data: v=5&type=info&arg=firefox&arg[]=chromium)
```

### 3.4 Error Handling
#### 3.4.1 Error Response Format
```typescript
interface ErrorResponse {
  error: string;
  resultcount: 0;
  results: [];
  type: "error";
  version: number | null;
}
```

#### 3.4.2 Error Scenarios
**Missing Version**:
```json
{
  "error": "Please specify an API version.",
  "resultcount": 0,
  "results": [],
  "type": "error", 
  "version": null
}
```

**Invalid Version** (non-v5):
```json
{
  "error": "Invalid version specified.",
  "resultcount": 0,
  "results": [],
  "type": "error",
  "version": 6
}
```

**Missing Request Type Or Data**:
```json
{
  "error": "No request type/data specified.",
  "resultcount": 0,
  "results": [],
  "type": "error",
  "version": 5
}
```

**Invalid Request Type**:
```json
{
  "error": "Incorrect request type specified.", 
  "resultcount": 0,
  "results": [],
  "type": "error",
  "version": 5
}
```

**Empty Search Query**:
```json
{
  "error": "Query arg too small.",
  "resultcount": 0,
  "results": [],
  "type": "error",
  "version": 5
}
```

**Invalid Search Field**:
```json
{
  "error": "Incorrect by field specified.",
  "resultcount": 0,
  "results": [],
  "type": "error",
  "version": 5
}
```

### 3.5 Implementation Architecture
**Request Processing Flow**:
1. Parse and validate request parameters
2. Route to appropriate service (search/info)
3. Execute database queries
4. Format and return response

### 3.6 Response Formats
#### Search Response
**Format**: Standard AUR search result format
```typescript
interface SearchResponse {
  resultcount: number;
  results: SearchResult[];
  type: "search";
  version: 5;
}

interface SearchResult {
  ID: number;               // Always 0 (placeholder)
  Name: string;             // Package name
  Description: string;      // Package description  
  PackageBase: string;      // Branch name (used as package base)
  PackageBaseID: number;    // Always 0 (placeholder)
  Version: string;          // epoch:pkgver-pkgrel or pkgver-pkgrel format
  URL: string;              // Package homepage URL
  URLPath: string;          // Snapshot download path (/cgit/aur.git/snapshot/{branch}.tar.gz)
  Maintainer: string;       // Always empty string
  NumVotes: number;         // Always 0 (placeholder)
  Popularity: number;       // Always 0 (placeholder)
  FirstSubmitted: number;   // Always 0 (placeholder)  
  LastModified: number;     // Always 0 (placeholder)
  OutOfDate: string | null; // Always null
}
```

#### Package Info Response  
**Format**: Detailed package information format
```typescript
interface InfoResponse {
  resultcount: number;
  results: PackageInfo[];
  type: "multiinfo";
  version: 5;
}

interface PackageInfo {
  ID: number;               // Always 0
  Name: string;             // Package name
  Description: string;      // Package description
  PackageBase: string;      // Branch name (used as package base)  
  PackageBaseID: number;    // Always 0
  Version: string;          // epoch:pkgver-pkgrel or pkgver-pkgrel format
  URL: string;              // Package homepage URL
  URLPath: string;          // Snapshot download path (/cgit/aur.git/snapshot/{branch}.tar.gz)
  Maintainer: string;       // Always empty string
  Submitter: string;        // Always empty string
  NumVotes: number;         // Always 0
  Popularity: number;       // Always 0
  FirstSubmitted: number;   // Always 0
  LastModified: number;     // Always 0
  OutOfDate: string | null; // Always null
  License: string[];        // Always empty array
  Depends: string[];        // Runtime dependencies (flattened across architectures)
  MakeDepends: string[];    // Build dependencies (flattened across architectures)  
  OptDepends: string[];     // Optional dependencies (flattened across architectures)
  CheckDepends: string[];   // Check dependencies (flattened across architectures)
  Provides: string[];       // Provided packages/features (flattened across architectures)
  Conflicts: string[];      // Conflicting packages (flattened across architectures)
  Replaces: string[];       // Replaced packages (flattened across architectures)
  Groups: string[];         // Package groups
  Keywords: string[];       // Always empty array
  CoMaintainers: string[];  // Always empty array
}
```

## Feature 4: CGit Snapshot Proxy

### 4.1 Snapshot Redirect Service
**Requirement**: Redirect package snapshot requests to GitHub archives
- **Purpose**: Provide AUR-compatible snapshot download URLs  
- **Method**: HTTP 302 temporary redirect to GitHub archive URLs
- **URL Pattern**: `/cgit/aur.git/snapshot/<branch_name>.tar.gz`

### 4.2 Redirect Logic
**URL Mapping**: 
```
/cgit/aur.git/snapshot/<branch_name>.tar.gz
→ https://github.com/archlinux/aur/archive/<commit_id>.tar.gz
```

Where `<commit_id>` is the latest commit ID for the requested branch.

**Error Handling**:
- Return 404 if branch not found in database
- Return 500 for database/service errors
- Validate `.tar.gz` suffix on snapshot name

## Feature 5: Git Repo Proxy

### 5.1 Virtual Repository Service
**Requirement**: Virtualize each AUR package branch as an independent Git repository
- **Purpose**: Enable `git clone` operations on individual packages without cloning entire AUR repository
- **Method**: Proxy Git protocol requests to GitHub while presenting each package as its own repository
- **URL Pattern**: `/<branch_name>` or `/<branch_name>.git`

### 5.2 Git Service Discovery
**Endpoint**: `GET /<branch_name>/info/refs?service=git-upload-pack`

**Response**:
```
Content-Type: application/x-git-upload-pack-advertisement

001e# service=git-upload-pack
000000e1<commit_id> HEAD\0multi_ack thin-pack side-band side-band-64k ofs-delta no-progress include-tag multi_ack_detailed no-done symref=HEAD:refs/heads/master object-format=sha1 agent=git/aur-mirror
003f<commit_id> refs/heads/master
0000
```

Where `<commit_id>` is the latest commit ID for the requested branch.

**Error Handling**:
- Return 404 if branch doesn't exist in database
- Return 403 "Please upgrade your git client." if `service` parameter missing
- Return 403 "Unsupported service" for services other than `git-upload-pack`

### 5.3 Git Upload Pack Proxy
**Endpoint**: `POST /<branch_name>/git-upload-pack`

**Protocol**: Git Smart HTTP Protocol

**Proxy Logic**:
```
/<branch_name>/git-upload-pack
→ https://github.com/archlinux/aur.git/git-upload-pack
```

**Implementation**: 
- Direct HTTP proxy to GitHub's AUR repository
- Forward all headers except `HOST` and `AUTHORIZATION`
- Add GitHub authentication if token is configured
- Stream request/response bodies for efficient handling
- Verify branch exists before proxying request

## Feature 6: Configuration Management
**Default Config File Location**: `~/.config/aur-mirror-meta/config.toml` (can be overridden via command line)

**Configuration Options**:
- `github_token`: Personal Access Token for GitHub API (optional but recommended)
- `db_path`: Custom database file path (optional, defaults to `~/.local/share/aur-mirror-meta/aur-meta.db`)

**Environment Variables**: (use if there is no value in config file)
- `AMM_GITHUB_TOKEN` / `GITHUB_TOKEN`: GitHub token
- `AMM_DB_PATH`: Database path
