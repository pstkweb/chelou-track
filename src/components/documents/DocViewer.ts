// Document viewer — PDF via PDF.js, images via <img>.
// Fed by the document* bucket from the scan (cf. ARCHITECTURE.md §10).
import { docUrl } from "../../lib/stream";
import type { DocumentRef } from "../../types/model";

export class DocViewer {
  private readonly container: HTMLElement;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  async open(doc: DocumentRef): Promise<void> {
    this.container.innerHTML = "";
    if (doc.kind === "pdf") {
      await this.openPdf(doc.file.fileId);
    } else {
      this.openImage(doc.file.fileId);
    }
  }

  private async openPdf(fileId: number): Promise<void> {
    // TODO: import * as pdfjsLib from 'pdfjs-dist'
    // TODO: set pdfjsLib.GlobalWorkerOptions.workerSrc
    // TODO: const pdf = await pdfjsLib.getDocument({ url: docUrl(fileId) }).promise
    // TODO: render each page into a <canvas> inside this.container
    const placeholder = document.createElement("p");
    placeholder.textContent = `PDF ${fileId} — TODO`;
    this.container.appendChild(placeholder);
    void docUrl(fileId);
  }

  private openImage(fileId: number): void {
    const img = document.createElement("img");
    img.src = docUrl(fileId);
    img.style.maxWidth = "100%";
    img.style.cursor = "zoom-in";
    // TODO: add pan/zoom on click
    this.container.appendChild(img);
  }

  close(): void {
    this.container.innerHTML = "";
  }
}
