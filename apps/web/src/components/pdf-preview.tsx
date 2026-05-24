"use client";

/**
 * PDFPreview — Componente de visualização de PDF com paginação e zoom.
 *
 * Carregado via next/dynamic (sem SSR) para evitar erros de servidor com
 * pdfjs-dist e canvas. Não integra no bundle inicial da página.
 *
 * Uso:
 *   const PDFPreview = dynamic(() => import("@/components/pdf-preview"), { ssr: false });
 *   <PDFPreview url="/path/to/file.pdf" />
 */

import { useState, useCallback } from "react";
import { Document, Page, pdfjs } from "react-pdf";
import "react-pdf/dist/Page/AnnotationLayer.css";
import "react-pdf/dist/Page/TextLayer.css";
import { Button } from "@/components/ui/button";
import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut, RotateCcw, ExternalLink } from "lucide-react";

// Configurar worker do pdfjs via CDN (evita bundling do worker no Next.js)
pdfjs.GlobalWorkerOptions.workerSrc = `//unpkg.com/pdfjs-dist@${pdfjs.version}/build/pdf.worker.min.mjs`;

interface PDFPreviewProps {
  url: string;
  className?: string;
  /** Escala inicial (1 = 100%) */
  initialScale?: number;
}

const SCALE_STEP = 0.2;
const MIN_SCALE = 0.5;
const MAX_SCALE = 3.0;

export default function PDFPreview({ url, className = "", initialScale = 1.0 }: PDFPreviewProps) {
  const [numPages, setNumPages] = useState<number>(0);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [scale, setScale] = useState<number>(initialScale);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const onDocumentLoadSuccess = useCallback(({ numPages }: { numPages: number }) => {
    setNumPages(numPages);
    setLoading(false);
  }, []);

  const onDocumentLoadError = useCallback((err: Error) => {
    setError(err.message);
    setLoading(false);
  }, []);

  const goToPrev = () => setCurrentPage((p) => Math.max(1, p - 1));
  const goToNext = () => setCurrentPage((p) => Math.min(numPages, p + 1));
  const zoomIn = () =>
    setScale((s) => Math.min(MAX_SCALE, parseFloat((s + SCALE_STEP).toFixed(1))));
  const zoomOut = () =>
    setScale((s) => Math.max(MIN_SCALE, parseFloat((s - SCALE_STEP).toFixed(1))));
  const resetZoom = () => setScale(initialScale);

  return (
    <div className={`flex flex-col gap-2 ${className}`}>
      {/* Toolbar */}
      <div className="flex items-center justify-between gap-2 rounded-md border bg-muted/40 px-3 py-1.5 text-sm">
        {/* Paginação */}
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={goToPrev}
            disabled={currentPage <= 1}
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
          <span className="min-w-[4rem] text-center text-muted-foreground">
            {loading ? "—" : `Pág. ${currentPage}/${numPages}`}
          </span>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={goToNext}
            disabled={currentPage >= numPages}
          >
            <ChevronRight className="h-4 w-4" />
          </Button>
        </div>

        {/* Zoom */}
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={zoomOut}
            disabled={scale <= MIN_SCALE}
          >
            <ZoomOut className="h-4 w-4" />
          </Button>
          <span className="min-w-[3rem] text-center text-muted-foreground">
            {Math.round(scale * 100)}%
          </span>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={zoomIn}
            disabled={scale >= MAX_SCALE}
          >
            <ZoomIn className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={resetZoom}
            title="Restaurar zoom"
          >
            <RotateCcw className="h-4 w-4" />
          </Button>
        </div>

        {/* Abrir em nova aba */}
        <Button variant="ghost" size="sm" asChild className="h-7 gap-1 text-xs">
          <a href={url} target="_blank" rel="noopener noreferrer">
            <ExternalLink className="h-3.5 w-3.5" />
            Abrir
          </a>
        </Button>
      </div>

      {/* Viewer */}
      <div className="relative overflow-auto rounded-md border bg-muted/20">
        {loading && (
          <div className="flex min-h-[320px] items-center justify-center text-muted-foreground">
            Carregando PDF…
          </div>
        )}
        {error && (
          <div className="flex min-h-[320px] items-center justify-center text-destructive">
            Erro ao carregar PDF: {error}
          </div>
        )}
        <Document
          file={url}
          onLoadSuccess={onDocumentLoadSuccess}
          onLoadError={onDocumentLoadError}
          className="flex justify-center p-2"
          loading=""
          error=""
        >
          <Page pageNumber={currentPage} scale={scale} renderAnnotationLayer renderTextLayer />
        </Document>
      </div>
    </div>
  );
}
