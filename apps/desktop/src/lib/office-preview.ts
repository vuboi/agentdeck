import * as XLSX from 'xlsx';
import type { OfficeFormat } from './files';

export type OfficeSheetPreview = {
  name: string;
  rows: string[][];
  truncated: boolean;
  rowCount: number;
  columnCount: number;
};

export type OfficeWorkbookPreview = { sheets: OfficeSheetPreview[] };

const MAX_PREVIEW_ROWS = 250;
const MAX_PREVIEW_COLUMNS = 32;

/** Reads a workbook into a compact, display-ready grid while bounding large sheets. */
export function parseOfficeWorkbook(bytes: Uint8Array): OfficeWorkbookPreview {
  const workbook = XLSX.read(bytes, {
    type: 'array',
    cellDates: true,
    dense: true,
  });
  const sheets = workbook.SheetNames.map((name) => {
    const sheet = workbook.Sheets[name];
    const range = sheet?.['!ref']
      ? XLSX.utils.decode_range(sheet['!ref'])
      : { s: { r: 0, c: 0 }, e: { r: -1, c: -1 } };
    const rowCount = Math.max(0, range.e.r - range.s.r + 1);
    const columnCount = Math.max(0, range.e.c - range.s.c + 1);
    const boundedRange = {
      s: range.s,
      e: {
        r: Math.min(range.e.r, range.s.r + MAX_PREVIEW_ROWS - 1),
        c: Math.min(range.e.c, range.s.c + MAX_PREVIEW_COLUMNS - 1),
      },
    };
    const rows =
      sheet && rowCount > 0 && columnCount > 0
        ? (
            XLSX.utils.sheet_to_json<unknown[]>(sheet, {
              header: 1,
              raw: false,
              defval: '',
              range: boundedRange,
              blankrows: true,
            }) as unknown[][]
          ).map((row) =>
            Array.from(
              { length: boundedRange.e.c - boundedRange.s.c + 1 },
              (_, index) => String(row[index] ?? ''),
            ),
          )
        : [];
    return {
      name,
      rows,
      truncated:
        rowCount > MAX_PREVIEW_ROWS || columnCount > MAX_PREVIEW_COLUMNS,
      rowCount,
      columnCount,
    };
  });
  return { sheets };
}

/** Renders a DOCX or PPTX into a viewer-owned element and returns its cleanup function. */
export async function renderOfficeDocument(
  format: Exclude<OfficeFormat, 'xlsx'>,
  bytes: Uint8Array,
  host: HTMLElement,
  scrollContainer: HTMLElement,
): Promise<() => void> {
  host.replaceChildren();
  if (format === 'docx') {
    const { renderAsync } = await import('docx-preview');
    await renderAsync(bytes, host, host, {
      inWrapper: true,
      breakPages: true,
      ignoreWidth: false,
      ignoreHeight: false,
      renderHeaders: true,
      renderFooters: true,
      renderFootnotes: true,
      renderEndnotes: true,
      renderComments: false,
      renderChanges: false,
      renderAltChunks: false,
      useBase64URL: true,
    });
    return () => host.replaceChildren();
  }

  const { PptxViewer, RECOMMENDED_ZIP_LIMITS } =
    await import('@aiden0z/pptx-renderer');
  const viewer = await PptxViewer.open(
    bytes.slice().buffer as ArrayBuffer,
    host,
    {
      fitMode: 'contain',
      renderMode: 'list',
      scrollContainer,
      zipLimits: RECOMMENDED_ZIP_LIMITS,
      lazySlides: true,
      lazyMedia: true,
      externalMedia: 'block',
      listOptions: {
        windowed: true,
        initialSlides: 3,
        batchSize: 3,
        showSlideLabels: true,
      },
    },
  );
  return () => viewer.destroy();
}
