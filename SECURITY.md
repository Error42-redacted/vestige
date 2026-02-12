# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.1.x   | :white_check_mark: |
| 1.0.x   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in Vestige, please report it responsibly:

1. **DO NOT** open a public GitHub issue
2. Email the maintainer directly (see GitHub profile)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

You can expect a response within 48 hours.

## Security Model

### Trust Boundaries

Vestige is a **local MCP server** designed to run on your machine with your user permissions:

- **Trusted**: The MCP client (Claude Code/Desktop) that connects via stdio
- **Untrusted**: Content passed through MCP tool arguments (validated before use)

### What Vestige Does NOT Do

- ❌ Make network requests (except first-run model download from Hugging Face)
- ❌ Execute shell commands
- ❌ Access files outside its data directory
- ❌ Send telemetry or analytics
- ❌ Phone home to any server

### Data Storage

All data is stored locally in SQLite:

| Platform | Location |
|----------|----------|
| macOS | `~/Library/Application Support/com.vestige.core/vestige.db` |
| Linux | `~/.local/share/vestige/core/vestige.db` |
| Windows | `%APPDATA%\vestige\core\vestige.db` |

**Default**: Data is stored in plaintext with owner-only file permissions (0600).

### Encryption at Rest

For database-level encryption, build with SQLCipher:
```bash
cargo build --no-default-features --features encryption,embeddings,vector-search
```
Set `VESTIGE_ENCRYPTION_KEY` environment variable. SQLCipher encrypts all database files including the WAL journal. Alternatively, use OS-level encryption (FileVault, BitLocker, LUKS).

### Input Validation

All MCP tool inputs are validated:

- Content size limit: 1MB max
- Query length limit: 1000 characters
- FTS5 queries are sanitized to prevent injection
- All SQL uses parameterized queries (`params![]` macro)

### Dependencies

We use well-maintained dependencies and run `cargo audit` regularly. Current status:

- **Vulnerabilities**: 0
- **Warnings**: 2 (unmaintained transitive dependencies with no known CVEs)

## Security Checklist

- [x] No hardcoded secrets
- [x] Parameterized SQL queries
- [x] Input validation on all tools
- [x] No command injection vectors
- [x] No unsafe Rust code
- [x] Dependencies audited
