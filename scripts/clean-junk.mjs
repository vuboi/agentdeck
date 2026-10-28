// Finds the files a work session leaves in the repo by accident (a Word or Excel file an agent made to try something, a
// screenshot, a log, a half-downloaded file) and removes them.
//
//   node scripts/clean-junk.mjs              list them (nothing is deleted)
//   node scripts/clean-junk.mjs --delete     delete them
//   node scripts/clean-junk.mjs --staged     fail when one of them is staged (used by the pre-commit hook)
//
// Only files git does not know (untracked and not ignored) are ever listed or deleted: a tracked file is never touched.
// Anything else untracked is shown as "review" and left alone, because it may be real work that is not committed yet.
import { execFileSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { resolve } from 'node:path';

const root = execFileSync('git', ['rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim();
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });

/** What is junk when it is not committed: documents and archives made to try something, logs, leftovers of a download. */
const JUNK = [
  /\.(docx?|xlsx?|pptx?|pdf|zip|7z|tar|gz)$/i,
  /\.(log|tmp|temp|part|orig|rej|bak|swp)$/i,
  /(^|\/)(nul|\.DS_Store|Thumbs\.db)$/i,
  // What the agents write while the demo media and thumbnails are checked.
  /^apps\/desktop\/src-tauri\/demo-media\/.+/,
  /(^|\/)(tmp|temp|scratch)\//i,
];
/** Never junk, whatever the name: the pictures and documents the app itself ships. */
const KEEP = [/^docs\//, /^apps\/desktop\/static\//, /^apps\/desktop\/src-tauri\/icons\//, /^README/i, /LICENSE/i];

const isJunk = (file) => !KEEP.some((pattern) => pattern.test(file)) && JUNK.some((pattern) => pattern.test(file));
const lines = (text) => text.split('\n').map((line) => line.trim()).filter(Boolean);

if (process.argv.includes('--staged')) {
  const staged = lines(git('diff', '--cached', '--name-only', '--diff-filter=A')).filter(isJunk);
  if (staged.length) {
    console.error(`These files look like leftovers and are staged to be added:\n${staged.map((file) => `  ${file}`).join('\n')}\nUnstage them (git restore --staged <file>) or delete them (node scripts/clean-junk.mjs --delete).`);
    process.exit(1);
  }
  process.exit(0);
}

const untracked = lines(git('ls-files', '--others', '--exclude-standard'));
// The files of the root that .gitignore hides (a Word file at the root is ignored so it is not committed by mistake, and
// would stay on the disk for ever): they are junk too.
const hidden = lines(git('ls-files', '--others', '--ignored', '--exclude-standard', '--', ':(glob)*'));
const junk = [...untracked, ...hidden].filter(isJunk);
const review = untracked.filter((file) => !isJunk(file));

if (junk.length === 0) console.log('No leftover files.');
else {
  console.log(`${process.argv.includes('--delete') ? 'Deleting' : 'Leftover files'} (${junk.length}):`);
  for (const file of junk) console.log(`  ${file}`);
}
if (review.length) console.log(`\nNot committed, kept (review them yourself):\n${review.map((file) => `  ${file}`).join('\n')}`);

if (process.argv.includes('--delete')) {
  for (const file of junk) rmSync(resolve(root, file), { force: true });
  if (junk.length) console.log(`\nDeleted ${junk.length} file${junk.length === 1 ? '' : 's'}.`);
} else if (junk.length) {
  console.log('\nRun with --delete to remove them.');
}
