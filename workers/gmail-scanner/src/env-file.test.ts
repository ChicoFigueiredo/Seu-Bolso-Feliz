import { describe, expect, it } from "vitest";
import { trocarVariavel } from "./env-file";

describe("trocarVariavel", () => {
  it("troca só a linha da variável e preserva o resto", () => {
    const antes = "# gmail\nGOOGLE_MAIL_CLIENT_ID=abc\nGMAIL_REFRESH_TOKEN=velho\nOUTRA=1\n";
    expect(trocarVariavel(antes, "GMAIL_REFRESH_TOKEN", "novo$1")).toBe(
      "# gmail\nGOOGLE_MAIL_CLIENT_ID=abc\nGMAIL_REFRESH_TOKEN=novo$1\nOUTRA=1\n",
    );
  });

  it("acrescenta no fim quando a variável não existe", () => {
    expect(trocarVariavel("A=1", "GMAIL_REFRESH_TOKEN", "x")).toBe("A=1\nGMAIL_REFRESH_TOKEN=x\n");
  });
});
