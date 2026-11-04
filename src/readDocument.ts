import { getDocument, GlobalWorkerOptions } from "pdfjs-dist";
import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";

GlobalWorkerOptions.workerSrc = workerSrc;

export async function readUploadedDocument(file: File): Promise<string> {
  const name = file.name.toLowerCase();
  let text = "";
  if (file.type === "application/pdf" || name.endsWith(".pdf")) {
    const data = new Uint8Array(await file.arrayBuffer());
    const doc = await getDocument({ data }).promise;
    const pages: string[] = [];
    const count = Math.min(doc.numPages, 20);
    for (let index = 1; index <= count; index += 1) {
      const page = await doc.getPage(index);
      const content = await page.getTextContent();
      const line = content.items
        .map((item) => ("str" in item ? item.str : ""))
        .join(" ")
        .replace(/\s+/g, " ")
        .trim();
      if (line) pages.push(line);
    }
    text = pages.join("\n\n");
  } else {
    text = await file.text();
  }
  const clipped = text.trim().slice(0, 20000);
  if (!clipped) throw new Error("That file had no readable text.");
  return `--- ${file.name} ---\n${clipped}`;
}
