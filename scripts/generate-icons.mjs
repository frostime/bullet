import fs from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);

// The interface and packaged icons share the same monochrome mark.
const mark = (await fs.readFile('public/logo.svg', 'utf8'))
  .replace(/<\/?svg[^>]*>/g, '').replace('currentColor', '#fff').trim();
const icon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <rect x="8" y="8" width="240" height="240" rx="64" fill="#171717"/>
  <g transform="translate(16 16) scale(1.75)">${mark}</g>
</svg>
`;
await fs.mkdir('tmp/icons', { recursive: true });
await fs.writeFile('tmp/.gitignore', '*\n');
await fs.writeFile('public/icon.svg', icon);
execFileSync(process.execPath, [require.resolve('@tauri-apps/cli/tauri.js'),
  'icon', 'public/icon.svg', '--output', 'tmp/icons'], { stdio: 'inherit' });
await fs.copyFile('tmp/icons/icon.png', 'src-tauri/icons/icon.png');
await fs.copyFile('tmp/icons/icon.ico', 'src-tauri/icons/icon.ico');
console.log('Updated public/icon.svg and src-tauri/icons/icon.{png,ico}');
