// PDF.js loader — cf. ARCHITECTURE.md §10 (viewer documents, indépendant de la synchro).

import type { PDFDocumentProxy } from 'pdfjs-dist';
import * as pdfjsLib from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { docUrl } from './stream';

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

/** Fetch a PDF through stream:// (never a raw pCloud URL) and parse it with PDF.js. */
export async function loadPdfDocument(fileId: number): Promise<PDFDocumentProxy> {
  const res = await fetch(docUrl(fileId));
  const data = await res.arrayBuffer();
  return pdfjsLib.getDocument({ data }).promise;
}
