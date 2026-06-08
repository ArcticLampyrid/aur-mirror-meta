# AUR Mirror Meta (AMM for short)
A system that builds on AUR GitHub Mirror and provides compatible endpoints (try our best) for AUR helper to interact with.

## Features
1. **AUR Metadata Fetching** - Fetches `.SRCINFO` data from GitHub AUR repository
2. **SRCINFO Parsing and Indexing** - Parses and indexes package metadata for fast searching
3. **Optional Official AUR Metadata Supplementation** - Fetches extra metadata from the official AUR package metadata export when available, with fallback source support and the ability to skip it entirely
4. **AUR RPC API** - Compatible endpoints for package search and information retrieval
5. **Snapshot Proxy** - Redirects package snapshot requests to GitHub archives
6. **Git Repo Proxy** - Virtualizes each AUR package branch as an independent Git repository

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
1. **Official AUR supplement is optional and best-effort** - AMM can now fetch metadata like `popularity`, `num_votes`, `maintainer`, `submitter`, `keywords`, and `co-maintainers` from the official AUR package metadata export, but this step is optional. If the supplement source is disabled, unavailable, or AUR is down, AMM will continue working with GitHub mirror data only.
2. **Listed/unlisted detection depends on supplement data** - When supplement data is available, AMM can filter out packages that appear to be unlisted. If supplement fetching is skipped or fails, this distinction cannot be made reliably.
3. **IPv4 is required for syncing GitHub mirror data** - GitHub still only supports IPv4, so you’ll need IPv4 connectivity, or at least a DNS64 + NAT64 setup.
4. **AUR cache may need manual cleanup** - Some AUR helpers may store URLs with `aur.archlinux.org` domain (eg. for Paru, all cloned repo under `~/.cache/paru/clone` store remote URLs with the original domain). If you switch to AMM, you may need to clear such cache manually.

## Installation
```bash
cargo build --release
```

## Usage
### Manually execution
```bash
# Login to GitHub with Personal Access Token
# (Optional, but recommended to avoid rate limiting)
aur-mirror-meta login --token your_pat

# Sync metadata from GitHub repository and index it
# By default, also try to fetch official AUR supplement metadata
aur-mirror-meta sync

# Sync without official AUR supplement metadata
# Useful when AUR is down or you want GitHub-mirror-only mode
aur-mirror-meta sync -s none

# Sync with fallback supplement sources
# AMM tries them in order until one succeeds
aur-mirror-meta sync \
  -s https://aur.archlinux.org/packages-meta-ext-v1.json.gz \
  -s /path/to/packages-meta-ext-v1.json.gz

# Start HTTP RPC server
# (Should be run after syncing)
aur-mirror-meta serve

# Show help
aur-mirror-meta --help
```

### Long-lived service
To run AMM as a long-lived service, with automatic syncing — see [Deploying with systemd](docs/systemd.md).

## Security
The config file stores your GitHub Personal Access Token (PAT) in plaintext. Please handle this file carefully to protect your credentials.

## Acknowledgements
This project is initiated via vibe coding. We appreciate the skillful models developed by Anthropic and OpenAI.
