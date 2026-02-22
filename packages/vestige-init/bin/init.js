#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

const HOME = os.homedir();
const PLATFORM = os.platform();

// ─── Branding ───────────────────────────────────────────────────────────────

const BANNER = `
  vestige init v2.0
  Give your AI a brain in 10 seconds.
  Now with 3D dashboard at localhost:3927/dashboard
`;

// ─── IDE Definitions ────────────────────────────────────────────────────────

const IDE_CONFIGS = {
  'Claude Code': {
    detect: () => {
      try {
        execSync('which claude', { stdio: 'ignore' });
        return true;
      } catch { return false; }
    },
    configPath: () => path.join(HOME, '.claude', 'settings.json'),
    format: 'claude-code',
    inject: (binaryPath) => {
      try {
        const result = execSync(`claude mcp add vestige "${binaryPath}" -s user 2>&1`, { encoding: 'utf8' });
        if (result.includes('already exists')) {
          console.log('  [skip] Claude Code — already configured');
          return false;
        }
        return true;
      } catch (err) {
        const msg = err.stdout || err.stderr || '';
        if (msg.includes('already exists')) {
          console.log('  [skip] Claude Code — already configured');
          return false;
        }
        return false;
      }
    },
  },

  'Claude Desktop': {
    detect: () => {
      const paths = {
        darwin: path.join(HOME, 'Library', 'Application Support', 'Claude', 'claude_desktop_config.json'),
        win32: path.join(process.env.APPDATA || '', 'Claude', 'claude_desktop_config.json'),
        linux: path.join(HOME, '.config', 'Claude', 'claude_desktop_config.json'),
      };
      return fs.existsSync(paths[PLATFORM] || '');
    },
    configPath: () => {
      const paths = {
        darwin: path.join(HOME, 'Library', 'Application Support', 'Claude', 'claude_desktop_config.json'),
        win32: path.join(process.env.APPDATA || '', 'Claude', 'claude_desktop_config.json'),
        linux: path.join(HOME, '.config', 'Claude', 'claude_desktop_config.json'),
      };
      return paths[PLATFORM];
    },
    format: 'mcpServers',
    key: 'mcpServers',
  },

  'Cursor': {
    detect: () => {
      const configPath = path.join(HOME, '.cursor', 'mcp.json');
      const appExists = PLATFORM === 'darwin'
        ? fs.existsSync('/Applications/Cursor.app')
        : fs.existsSync(path.join(HOME, '.cursor'));
      return appExists || fs.existsSync(configPath);
    },
    configPath: () => path.join(HOME, '.cursor', 'mcp.json'),
    format: 'mcpServers',
    key: 'mcpServers',
  },

  'VS Code (Copilot)': {
    detect: () => {
      try {
        execSync('which code', { stdio: 'ignore' });
        return true;
      } catch {
        return PLATFORM === 'darwin' && fs.existsSync('/Applications/Visual Studio Code.app');
      }
    },
    configPath: () => {
      // User-level MCP config
      const paths = {
        darwin: path.join(HOME, 'Library', 'Application Support', 'Code', 'User', 'settings.json'),
        win32: path.join(process.env.APPDATA || '', 'Code', 'User', 'settings.json'),
        linux: path.join(HOME, '.config', 'Code', 'User', 'settings.json'),
      };
      return paths[PLATFORM];
    },
    format: 'vscode',
    key: 'servers',
    note: 'Tip: For project-level config, create .vscode/mcp.json with {"servers": {"vestige": ...}}',
  },

  'Xcode 26.3': {
    detect: () => {
      if (PLATFORM !== 'darwin') return false;
      return fs.existsSync('/Applications/Xcode.app') ||
             fs.existsSync(path.join(HOME, 'Library', 'Developer', 'Xcode'));
    },
    configPath: () => path.join(HOME, 'Library', 'Developer', 'Xcode', 'CodingAssistant', 'ClaudeAgentConfig', '.claude'),
    format: 'xcode',
    key: 'projects',
  },

  'JetBrains (Junie)': {
    detect: () => {
      const jetbrainsDir = PLATFORM === 'darwin'
        ? path.join(HOME, 'Library', 'Application Support', 'JetBrains')
        : path.join(HOME, '.config', 'JetBrains');
      return fs.existsSync(jetbrainsDir);
    },
    configPath: () => path.join(HOME, '.junie', 'mcp', 'mcp.json'),
    format: 'mcpServers',
    key: 'mcpServers',
  },

  'Windsurf': {
    detect: () => {
      const configPath = PLATFORM === 'win32'
        ? path.join(HOME, '.codeium', 'windsurf', 'mcp_config.json')
        : path.join(HOME, '.codeium', 'windsurf', 'mcp_config.json');
      return fs.existsSync(configPath) ||
             (PLATFORM === 'darwin' && fs.existsSync('/Applications/Windsurf.app'));
    },
    configPath: () => path.join(HOME, '.codeium', 'windsurf', 'mcp_config.json'),
    format: 'mcpServers',
    key: 'mcpServers',
  },
};

// ─── Helpers ────────────────────────────────────────────────────────────────

function findBinary() {
  // Check common locations
  const candidates = [
    path.join('/usr', 'local', 'bin', 'vestige-mcp'),
    path.join(HOME, '.cargo', 'bin', 'vestige-mcp'),
    // npm global install location
    (() => {
      try {
        const npmPrefix = execSync('npm prefix -g', { encoding: 'utf8' }).trim();
        return path.join(npmPrefix, 'bin', 'vestige-mcp');
      } catch { return null; }
    })(),
  ].filter(Boolean);

  // Also try which/where
  try {
    const result = execSync(PLATFORM === 'win32' ? 'where vestige-mcp' : 'which vestige-mcp', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'ignore'],
    }).trim();
    if (result) candidates.unshift(result);
  } catch {}

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

function readJsonSafe(filePath) {
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    return JSON.parse(content);
  } catch {
    return null;
  }
}

function ensureDir(filePath) {
  const dir = path.dirname(filePath);
  if (!dir || dir === '.') return;
  fs.mkdirSync(dir, { recursive: true });
}

function buildVestigeConfig(binaryPath) {
  return {
    command: binaryPath,
    args: [],
    env: {},
  };
}

function buildXcodeConfig(binaryPath) {
  return {
    projects: {
      '*': {
        mcpServers: {
          vestige: {
            type: 'stdio',
            command: binaryPath,
            args: [],
            env: {
              PATH: '/usr/local/bin:/usr/bin:/bin',
            },
          },
        },
        hasTrustDialogAccepted: true,
      },
    },
  };
}

function injectConfig(ide, ideName, binaryPath) {
  const configPath = ide.configPath();
  if (!configPath) return false;

  // Claude Code uses its own CLI
  if (ide.format === 'claude-code') {
    return ide.inject(binaryPath);
  }

  ensureDir(configPath);
  let config = readJsonSafe(configPath) || {};

  if (ide.format === 'xcode') {
    // Xcode has a different structure
    const xcodeConfig = buildXcodeConfig(binaryPath);
    if (config.projects && config.projects['*'] && config.projects['*'].mcpServers && config.projects['*'].mcpServers.vestige) {
      console.log(`  [skip] ${ideName} — already configured`);
      return false;
    }
    // Merge with existing config
    if (!config.projects) config.projects = {};
    if (!config.projects['*']) config.projects['*'] = {};
    if (!config.projects['*'].mcpServers) config.projects['*'].mcpServers = {};
    config.projects['*'].mcpServers.vestige = xcodeConfig.projects['*'].mcpServers.vestige;
    config.projects['*'].hasTrustDialogAccepted = true;
  } else if (ide.format === 'vscode') {
    // VS Code uses "mcp" key in settings.json with "servers" subkey
    if (!config.mcp) config.mcp = {};
    if (!config.mcp.servers) config.mcp.servers = {};
    if (config.mcp.servers.vestige) {
      console.log(`  [skip] ${ideName} — already configured`);
      return false;
    }
    config.mcp.servers.vestige = buildVestigeConfig(binaryPath);
  } else {
    // Standard mcpServers format (Cursor, Claude Desktop, JetBrains, Windsurf)
    const key = ide.key || 'mcpServers';
    if (!config[key]) config[key] = {};
    if (config[key].vestige) {
      console.log(`  [skip] ${ideName} — already configured`);
      return false;
    }
    config[key].vestige = buildVestigeConfig(binaryPath);
  }

  fs.writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
  return true;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  console.log(BANNER);

  // Step 1: Find the binary
  console.log('Looking for vestige-mcp binary...');
  const binaryPath = findBinary();

  if (!binaryPath) {
    console.log('');
    console.log('vestige-mcp not found. Installing...');
    console.log('');
    console.log('Install manually:');
    console.log('');
    console.log('  # macOS (Apple Silicon)');
    console.log('  curl -L https://github.com/samvallad33/vestige/releases/latest/download/vestige-mcp-aarch64-apple-darwin.tar.gz | tar -xz');
    console.log('  sudo mv vestige-mcp vestige vestige-restore /usr/local/bin/');
    console.log('');
    console.log('  # Or via npm');
    console.log('  npm install -g vestige-mcp-server');
    console.log('');
    console.log('Then run: npx @vestige/init');
    process.exit(1);
  }

  console.log(`  Found: ${binaryPath}`);
  console.log('');

  // Step 2: Detect IDEs
  console.log('Scanning for IDEs...');
  const detected = [];
  const notFound = [];

  for (const [name, ide] of Object.entries(IDE_CONFIGS)) {
    if (ide.detect()) {
      detected.push({ name, ide });
      console.log(`  [found] ${name}`);
    } else {
      notFound.push(name);
    }
  }

  if (detected.length === 0) {
    console.log('  No supported IDEs found.');
    console.log('');
    console.log('Supported: Claude Code, Claude Desktop, Cursor, VS Code, Xcode, JetBrains, Windsurf');
    process.exit(1);
  }

  console.log('');

  // Step 3: Inject configs
  console.log('Configuring Vestige...');
  let configured = 0;
  let skipped = 0;

  for (const { name, ide } of detected) {
    try {
      const injected = injectConfig(ide, name, binaryPath);
      if (injected) {
        console.log(`  [done] ${name}`);
        configured++;
        if (ide.note) {
          console.log(`         ${ide.note}`);
        }
      } else {
        skipped++;
      }
    } catch (err) {
      console.log(`  [fail] ${name} — ${err.message}`);
    }
  }

  console.log('');

  // Step 4: Summary
  if (configured > 0) {
    console.log(`Vestige configured for ${configured} IDE${configured > 1 ? 's' : ''}.${skipped > 0 ? ` (${skipped} already configured)` : ''}`);
    console.log('');
    console.log('Next steps:');
    console.log('  1. Restart your IDE(s)');
    console.log('  2. Ask your AI: "Remember that I prefer TypeScript over JavaScript"');
    console.log('  3. New session: "What are my coding preferences?"');
    console.log('');
    console.log('Your AI has a brain now.');
    console.log('');
    console.log('  Dashboard: http://localhost:3927/dashboard');
  } else {
    console.log('All detected IDEs already have Vestige configured.');
  }

  console.log('');
  console.log('Docs: https://github.com/samvallad33/vestige');
}

main();
