import { plannedProvider } from './provider';

// Any S3-compatible bucket: AWS S3, Cloudflare R2, Backblaze B2, MinIO on a home server.
export const s3 = plannedProvider({
  id: 's3',
  label: 'S3-compatible bucket',
  icon: 'lucideDatabase',
  summary: 'Objects in a bucket you own (R2, B2, MinIO, S3). Cheap, no per-user limits, works from any device.',
  fields: [
    { key: 'endpoint', label: 'Endpoint', placeholder: 'https://<account>.r2.cloudflarestorage.com' },
    { key: 'bucket', label: 'Bucket', placeholder: 'agentdeck-history' },
    { key: 'accessKeyId', label: 'Access key id' },
    { key: 'secretAccessKey', label: 'Secret access key', secret: true },
  ],
  todo: [
    'Transport: Rust command `sync_s3` with a small SigV4 signer on `reqwest` (or the `aws-sdk-s3` crate if the build size is acceptable). No credentials in the webview.',
    'Secrets: the secret key goes to the OS keyring; only the key id, endpoint and bucket sit in prefs.',
    'Layout: `<deviceId>/<chatId>/<yyyy-mm>.jsonl` objects plus `<deviceId>/sessions.json`. One prefix per device avoids write races without needing locks.',
    'push(batch): GET the object (If-None-Match for new ones), merge by record id, PUT with `If-Match` on the ETag so a concurrent writer is detected, retry once.',
    'pull(since): `ListObjectsV2` per other device prefix, keep objects with `LastModified` after the cursor, download, return an ArchiveBatch.',
    'Optional server-side encryption header, and client-side encryption shared with the other providers (`sync/privacy.ts`).',
    'UI: the four fields, "Test connection" (HEAD bucket, then a put/delete of a probe object), "Sync now".',
  ],
});
