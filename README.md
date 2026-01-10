# AUR Mirror Meta (AMM for short)
A system that builds on AUR GitHub Mirror and provides compatible endpoints (try our best) for AUR helper to interact with.

## Features
1. **AUR Metadata Fetching** - Fetches `.SRCINFO` data from GitHub AUR repository
2. **SRCINFO Parsing and Indexing** - Parses and indexes package metadata for fast searching
3. **AUR RPC API** - Compatible endpoints for package search and information retrieval
4. **Snapshot Proxy** - Redirects package snapshot requests to GitHub archives
5. **Git Repo Proxy** - Virtualizes each AUR package branch as an independent Git repository

## Details
See [Product Requirements Document (PRD)](PRD.md) for more information.

## Compatibility
Test with [Paru](https://github.com/Morganamilo/paru) and it works well:
```bash
paru --aururl http://localhost:3000
```

For other AUR helpers, the compatibility may vary.

**This project is in early development stages and may not cover all cases.**

## Known Limitations
1. **Some metadata is missing** - Due to GitHub AUR Mirror limitations, metadata like `popularity`, `num_votes`, and `maintainer` is not available.
2. **Unlisted packages are included** - Also due to GitHub AUR Mirror limitations, we cannot distinguish between listed and unlisted packages.
3. **IPv4 is required for syncing** - GitHub still only supports IPv4, so you’ll need IPv4 connectivity, or at least a DNS64 + NAT64 setup.
4. **AUR cache may need manual cleanup** - Some AUR helpers may store URLs with `aur.archlinux.org` domain (eg. for Paru, all cloned repo under `~/.cache/paru/clone` store remote URLs with the original domain). If you switch to AMM, you may need to clear such cache manually.

## Installation
```bash
cargo build --release
```

## Usage
```bash
# Login to GitHub with Personal Access Token
# (Optional, but recommended to avoid rate limiting)
aur-mirror-meta login --token your_pat

# Sync metadata from GitHub repository and index it
aur-mirror-meta sync

# Start HTTP RPC server
# (Should be run after syncing)
aur-mirror-meta serve

# Show help
aur-mirror-meta --help
```

## Security
The config file stores your GitHub Personal Access Token (PAT) in plaintext. Please handle this file carefully to protect your credentials.

## Acknowledgements
This project is initiated via vibe coding. We appreciate the skillful models developed by Anthropic and OpenAI.
