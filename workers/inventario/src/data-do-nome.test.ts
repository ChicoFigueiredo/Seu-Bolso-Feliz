import { describe, expect, it } from "vitest";
import { dataDoNome } from "./data-do-nome";

describe("dataDoNome", () => {
  it("lê os nomes do WhatsApp", () => {
    expect(dataDoNome("IMG-20261104-WA0019.jpg")).toBe("2026-11-04");
    expect(dataDoNome("WhatsApp Image 2026-11-04 at 10.15.32.jpeg")).toBe("2026-11-04");
    expect(dataDoNome("/espelho/WhatsApp Images/Sent/IMG-20251001-WA0358.jpg")).toBe("2025-10-01");
  });

  it("lê capturas de tela e câmera", () => {
    expect(dataDoNome("Screenshot_2026-10-26-10-15-32-123_com.nu.production.png")).toBe(
      "2026-10-26",
    );
    expect(dataDoNome("Screenshot_20261026_101532.jpg")).toBe("2026-10-26");
    expect(dataDoNome("IMG_20261026_101532.jpg")).toBe("2026-10-26");
    expect(dataDoNome("PXL_20261026_101532123.jpg")).toBe("2026-10-26");
  });

  it("recusa número que não é data", () => {
    expect(dataDoNome("comprovante.jpg")).toBeNull();
    expect(dataDoNome("IMG-20261399-WA0001.jpg")).toBeNull();
    expect(dataDoNome("foto 123456789.png")).toBeNull();
  });
});
