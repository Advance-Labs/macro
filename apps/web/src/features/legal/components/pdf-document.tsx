import PdfJsWorker from '@block-pdf/PdfViewer/pdfjs-worker?worker';
import type {
  PDFDocumentProxy,
  RenderTask,
} from 'pdfjs-dist/types/src/display/api';
import { createEffect, createSignal, Index, onCleanup, Show } from 'solid-js';
import { type Field, fieldLabels } from '../core/models';

export function PdfDocument(props: {
  bytes: Uint8Array;
  page: number;
  fields: Field[];
  mode?: 'place' | 'sign' | 'read';
  selectedId?: string;
  onPlace?: (x: number, y: number) => void;
  onSelect?: (field: Field) => void;
  onMove?: (id: string, x: number, y: number) => void;
  values?: Record<string, string>;
}) {
  let canvas!: HTMLCanvasElement;
  let sheet!: HTMLDivElement;
  const [document, setDocument] = createSignal<PDFDocumentProxy>();
  const [error, setError] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [ratio, setRatio] = createSignal(0.773);
  createEffect(() => {
    const bytes = props.bytes;
    let disposed = false;
    let pdf: PDFDocumentProxy | undefined;
    async function load() {
      try {
        const { getDocument, GlobalWorkerOptions } = await import('pdfjs-dist');
        if (!GlobalWorkerOptions.workerPort)
          GlobalWorkerOptions.workerPort = new PdfJsWorker();
        pdf = await getDocument({ data: bytes.slice(), isEvalSupported: false })
          .promise;
        if (disposed) {
          await pdf.destroy();
          return;
        }
        setDocument(pdf);
      } catch {
        if (!disposed)
          setError('Could not display this PDF. Try exporting it again.');
      }
    }
    void load();
    onCleanup(() => {
      disposed = true;
      void pdf?.destroy();
    });
  });
  createEffect(() => {
    const pdf = document();
    const number = props.page;
    if (!pdf) return;
    let disposed = false;
    let task: RenderTask | undefined;
    async function render() {
      setLoading(true);
      try {
        const page = await pdf!.getPage(number);
        if (disposed) return;
        const original = page.getViewport({ scale: 1 });
        setRatio(original.width / original.height);
        const viewport = page.getViewport({ scale: 1240 / original.width });
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        const context = canvas.getContext('2d');
        if (!context) return;
        task = page.render({ canvasContext: context, viewport });
        await task.promise;
        if (!disposed) setLoading(false);
      } catch {
        if (!disposed) {
          setLoading(false);
          setError('Could not render this page.');
        }
      }
    }
    void render();
    onCleanup(() => {
      disposed = true;
      task?.cancel();
    });
  });
  function beginDrag(event: PointerEvent, field: Field) {
    if (props.mode !== 'place') return;
    event.preventDefault();
    event.stopPropagation();
    props.onSelect?.(field);
    const target = event.currentTarget as HTMLElement;
    const bounds = sheet.getBoundingClientRect();
    const start = { x: event.clientX, y: event.clientY };
    target.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) =>
      props.onMove?.(
        field.id,
        Math.max(
          0,
          Math.min(
            1 - field.width,
            field.x + (e.clientX - start.x) / bounds.width
          )
        ),
        Math.max(
          0,
          Math.min(
            1 - field.height,
            field.y + (e.clientY - start.y) / bounds.height
          )
        )
      );
    const end = () => {
      target.removeEventListener('pointermove', move);
      target.removeEventListener('pointerup', end);
      target.removeEventListener('pointercancel', end);
    };
    target.addEventListener('pointermove', move);
    target.addEventListener('pointerup', end);
    target.addEventListener('pointercancel', end);
  }
  return (
    <div class="w-full max-w-[620px] mx-auto">
      <Show when={error()}>
        <p role="alert" class="text-failure p-4">
          {error()}
        </p>
      </Show>
      <div
        ref={sheet}
        class="relative shadow-md border border-edge-muted overflow-hidden"
        style={{ 'aspect-ratio': `${ratio()}` }}
        onClick={(event) => {
          if (props.mode !== 'place' || event.target !== canvas) return;
          const bounds = sheet.getBoundingClientRect();
          props.onPlace?.(
            (event.clientX - bounds.left) / bounds.width,
            (event.clientY - bounds.top) / bounds.height
          );
        }}
      >
        <canvas
          ref={canvas}
          class="w-full h-full block"
          aria-label={`Document page ${props.page}`}
        />
        <Show when={loading()}>
          <div class="absolute inset-0 grid place-items-center bg-surface/80 text-ink-muted">
            Loading document…
          </div>
        </Show>
        <Index each={props.fields.filter((field) => field.page === props.page)}>
          {(field) => (
            <button
              type="button"
              class="absolute flex items-center px-2 overflow-hidden text-left text-xs border-2 border-accent text-accent bg-accent-bg/90 touch-none focus-visible:ring-2 focus-visible:ring-edge-focus"
              classList={{
                'ring-2 ring-edge-focus': props.selectedId === field().id,
                'font-serif italic text-lg':
                  field().kind === 'signature' &&
                  !!(props.values?.[field().id] || field().value),
              }}
              style={{
                left: `${field().x * 100}%`,
                top: `${field().y * 100}%`,
                width: `${field().width * 100}%`,
                height: `${field().height * 100}%`,
              }}
              aria-label={`${fieldLabels[field().kind]}${field().required ? ', required' : ''}`}
              onPointerDown={(event) => beginDrag(event, field())}
              onClick={(event) => {
                event.stopPropagation();
                props.onSelect?.(field());
              }}
            >
              {props.values?.[field().id] ||
                field().value ||
                fieldLabels[field().kind]}
              {field().required && !field().value && !props.values?.[field().id]
                ? ' *'
                : ''}
            </button>
          )}
        </Index>
      </div>
    </div>
  );
}
