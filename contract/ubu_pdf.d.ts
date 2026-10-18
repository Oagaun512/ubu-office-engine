/* tslint:disable */
/* eslint-disable */
export function engineVersion(): string;
export class PageSize {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  readonly widthPt: number;
  readonly heightPt: number;
}
export class PdfDocument {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  static open(bytes: Uint8Array, password?: string | null): PdfDocument;
  pageSize(page: number): PageSize;
  renderPage(page: number, scale: number): RenderedPage;
  pageText(page: number): string;
  save(): Uint8Array;
  readonly pageCount: number;
}
export class RenderedPage {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  rgba(): Uint8ClampedArray;
  warnings(): string[];
  readonly width: number;
  readonly height: number;
}
