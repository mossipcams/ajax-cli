import { expect, test, type Page } from "@playwright/test";
import {
  DETAIL_FIXTURE,
  emulateCoarsePointer,
  mockFetch,
  sessionEventJson,
  sessionSnapshotJson,
} from "./fixtures";

const MAX_FRAME_BYTES = 8 * 1024 * 1024;
const ATTACHMENT_TOO_LARGE =
  "That attachment is too large to send even after compression. Remove it or choose a smaller file.";

type CapturedPrompt = {
  text: string;
  contentBlocks?: Array<{ type: string; data?: string; mimeType?: string }>;
  frameBytes: number;
};

async function openSessionChatWithImage(
  page: Page,
  onPrompt?: (prompt: CapturedPrompt) => void,
) {
  await emulateCoarsePointer(page);
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, session_capable: true, agent: "Cursor" },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as {
        type?: string;
        text?: string;
        contentBlocks?: CapturedPrompt["contentBlocks"];
        clientMessageId?: string;
      };
      if (event.type === "prompt") {
        onPrompt?.({
          text: event.text ?? "",
          contentBlocks: event.contentBlocks,
          frameBytes: new TextEncoder().encode(message).length,
        });
        if (event.clientMessageId) {
          socket.send(
            sessionEventJson(0, {
              type: "prompt_accepted",
              clientMessageId: event.clientMessageId,
            }),
          );
        }
      }
    });
    socket.send(
      sessionSnapshotJson({
        cursor: 0,
        model: "auto",
        turnState: "idle",
        promptCapabilities: { image: true },
      }),
    );
  });
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
}

async function createLargeJpeg(page: Page, minBytes: number) {
  const payload = await page.evaluate(async (targetBytes) => {
    const canvas = document.createElement("canvas");
    canvas.width = 2600;
    canvas.height = 2000;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no canvas context");
    const gradient = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
    gradient.addColorStop(0, "#e74c3c");
    gradient.addColorStop(1, "#3498db");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < 8000; i += 1) {
      ctx.fillStyle = `hsl(${i % 360} 80% 50%)`;
      ctx.fillRect(Math.random() * canvas.width, Math.random() * canvas.height, 6, 6);
    }
    let quality = 0.95;
    let bytes: Uint8Array | null = null;
    while (quality >= 0.5) {
      const blob = await new Promise<Blob>((resolve, reject) => {
        canvas.toBlob((value) => (value ? resolve(value) : reject(new Error("no blob"))), "image/jpeg", quality);
      });
      const buffer = new Uint8Array(await blob.arrayBuffer());
      bytes = buffer;
      if (buffer.byteLength >= targetBytes) break;
      quality -= 0.05;
    }
    if (!bytes) throw new Error("could not encode jpeg");
    return { bytes: Array.from(bytes), size: bytes.byteLength };
  }, minBytes);
  expect(payload.size).toBeGreaterThan(minBytes);
  return Buffer.from(payload.bytes);
}

test("large photo sends an ACP image block within the bounded frame", async ({ page }) => {
  const prompts: CapturedPrompt[] = [];
  await openSessionChatWithImage(page, (prompt) => prompts.push(prompt));

  const jpeg = await createLargeJpeg(page, 256 * 1024 + 1);
  await page.locator(".session-composer-attach-input").setInputFiles({
    name: "large-photo.jpg",
    mimeType: "image/jpeg",
    buffer: jpeg,
  });
  await expect(page.getByTestId("session-composer-attachments")).toContainText("large-photo.jpg");

  await page.getByLabel("Message").fill("describe this photo");
  await page.getByLabel("Message").press("Enter");

  await expect.poll(() => prompts.length).toBe(1);
  const prompt = prompts[0]!;
  expect(prompt.text).toBe("describe this photo");
  expect(prompt.contentBlocks?.some((block) => block.type === "image")).toBe(true);
  const imageBlock = prompt.contentBlocks?.find((block) => block.type === "image");
  expect(imageBlock?.mimeType).toMatch(/^image\//);
  expect((imageBlock?.data?.length ?? 0)).toBeGreaterThan(0);
  expect(prompt.frameBytes).toBeLessThanOrEqual(MAX_FRAME_BYTES);
});

test("impossible-fit attachment shows an error and does not dispatch", async ({ page }) => {
  const prompts: CapturedPrompt[] = [];
  const oversizeBase64Chars = MAX_FRAME_BYTES + 64 * 1024;
  await page.addInitScript((frameBytes) => {
    const originalToDataUrl = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function (...args) {
      if ((window as unknown as { __forceOversizedImage?: boolean }).__forceOversizedImage) {
        return `data:image/jpeg;base64,${"A".repeat(frameBytes)}`;
      }
      return originalToDataUrl.apply(this, args);
    };
    const originalReadAsDataUrl = FileReader.prototype.readAsDataURL;
    FileReader.prototype.readAsDataURL = function (blob: Blob) {
      if ((window as unknown as { __forceOversizedImage?: boolean }).__forceOversizedImage) {
        queueMicrotask(() => {
          Object.defineProperty(this, "result", {
            value: `data:image/jpeg;base64,${"A".repeat(frameBytes)}`,
          });
          this.onload?.(new ProgressEvent("load"));
        });
        return;
      }
      return originalReadAsDataUrl.call(this, blob);
    };
  }, oversizeBase64Chars);
  await openSessionChatWithImage(page, (prompt) => prompts.push(prompt));

  await page.evaluate(() => {
    (window as unknown as { __forceOversizedImage?: boolean }).__forceOversizedImage = true;
  });

  const jpeg = await createLargeJpeg(page, 256 * 1024 + 1);
  await page.locator(".session-composer-attach-input").setInputFiles({
    name: "large-photo.jpg",
    mimeType: "image/jpeg",
    buffer: jpeg,
  });
  await expect(page.getByTestId("session-composer-attachments")).toContainText("large-photo.jpg");

  await page.getByLabel("Message").fill("should fail");
  await page.getByLabel("Message").press("Enter");

  await expect(page.locator(".session-composer-attachment-error")).toContainText(
    ATTACHMENT_TOO_LARGE,
    { timeout: 10_000 },
  );
  await expect.poll(() => prompts.length).toBe(0);
});
