import { describe, expect, it } from "vitest";
import {
  classificarMovimento,
  ehTitular,
  limpar,
  type EntradaMovimento,
} from "./pluggy-normalizar";

function mov(p: Partial<EntradaMovimento>): EntradaMovimento {
  return {
    descricao_original: null,
    supplier_name_raw: null,
    tipo: "debito",
    valor_centavos: -10000,
    conta_provavel: "CAIXA · 1",
    categoria_provavel: null,
    ...p,
  };
}

describe("tipo", () => {
  it("poupança integrada é transferência interna nos dois lados", () => {
    expect(
      classificarMovimento(mov({ descricao_original: "APLICACAO POUPANCA INTEGR" })).tipo,
    ).toBe("transferencia_interna");
    expect(
      classificarMovimento(
        mov({
          descricao_original: "RESGATE POUPANCA INTEGRAD",
          valor_centavos: 5000,
          tipo: "credito",
        }),
      ).tipo,
    ).toBe("transferencia_interna");
  });

  it("aplicação em CDB é investimento", () => {
    expect(classificarMovimento(mov({ descricao_original: "APLICACAO CDB DI" })).tipo).toBe(
      "investimento",
    );
  });

  it("fatura paga: lado da conta e lado do cartão", () => {
    expect(classificarMovimento(mov({ descricao_original: "Pagamento de fatura" })).tipo).toBe(
      "pagamento_fatura",
    );
    expect(classificarMovimento(mov({ descricao_original: "PGTO FAT CARTAO C6" })).tipo).toBe(
      "pagamento_fatura",
    );
    expect(
      classificarMovimento(
        mov({ descricao_original: "C6 BANK", supplier_name_raw: "BANCO C6 S.A." }),
      ).tipo,
    ).toBe("pagamento_fatura");
    expect(
      classificarMovimento(
        mov({ descricao_original: "Pagamento recebido", tipo: "estorno", valor_centavos: 240632 }),
      ).tipo,
    ).toBe("pagamento_fatura");
    expect(
      classificarMovimento(
        mov({ descricao_original: "Inclusao de Pagamento Ciclo Corrente", valor_centavos: 99000 }),
      ).tipo,
    ).toBe("pagamento_fatura");
  });

  it("transferência para o próprio titular, mas não para homônimos parciais", () => {
    expect(
      classificarMovimento(
        mov({
          descricao_original: "Transferência enviada",
          supplier_name_raw: "FRANCISCO LIMA FIGUEIREDO",
        }),
      ).tipo,
    ).toBe("transferencia_interna");
    expect(
      classificarMovimento(
        mov({ descricao_original: "Pix recebido de CHICO FIGUEIREDO", valor_centavos: 5000 }),
      ).tipo,
    ).toBe("transferencia_interna");
    expect(
      classificarMovimento(
        mov({ descricao_original: "Pix enviado para Francisco Lima Figueiredo" }),
      ).tipo,
    ).toBe("transferencia_interna");
    expect(
      classificarMovimento(
        mov({
          descricao_original: "PIX ENVIADO",
          supplier_name_raw: "FRANCISCA MARIA DA SILVA SOUSA",
        }),
      ).tipo,
    ).toBeNull();
    expect(
      classificarMovimento(
        mov({
          descricao_original: "Pix enviado para 42.200.415 JOSE FRANCISCO VIEIRA DE REZENDE NETO",
        }),
      ).tipo,
    ).toBeNull();
    expect(ehTitular(limpar("Mayara Barros de Souza Figueiredo"))).toBe(false);
  });

  it("devolução na conta é estorno; IOF e juros ficam como estão", () => {
    expect(
      classificarMovimento(
        mov({ descricao_original: "DEVOLUCAO PIX RECEBIDO", valor_centavos: 3000 }),
      ).tipo,
    ).toBe("estorno");
    const iof = classificarMovimento(
      mov({ descricao_original: "IOF de compra internacional", tipo: "compra" }),
    );
    expect(iof).toMatchObject({ tipo: null, fornecedor_normalizado: "IOF" });
    expect(
      classificarMovimento(mov({ descricao_original: "Juros de atraso", tipo: "compra" })).tipo,
    ).toBeNull();
    expect(classificarMovimento(mov({ descricao_original: "JUROS LIMITE CONTA" })).tipo).toBeNull();
  });

  it("PIX comum a terceiro não é reclassificado", () => {
    expect(
      classificarMovimento(
        mov({
          descricao_original: "PIX ENVIADO",
          supplier_name_raw: "VALLE LOCACOES E EVENTOS LTDA",
        }),
      ).tipo,
    ).toBeNull();
  });
});

describe("fornecedor_normalizado", () => {
  const forn = (descricao: string, fornecedor: string | null = null) =>
    classificarMovimento(
      mov({ descricao_original: descricao, supplier_name_raw: fornecedor, tipo: "compra" }),
    ).fornecedor_normalizado;

  it("tira cidade e país das colunas do cartão", () => {
    expect(forn("MIX LIGHT              BRASILIA      BRA")).toBe("MIX LIGHT");
    expect(forn("ESCOVEI AGUAS CLARAS   BRASILIA      BRA")).toBe("ESCOVEI AGUAS CLARAS");
    expect(forn("PRODUTOSUOL            SAO PAULO     BRA")).toBe("PRODUTOSUOL");
  });

  it("agrupa marcas conhecidas", () => {
    expect(
      forn("UBER UBER * PENDING    SAO PAULO     BRA", "UBER DO BRASIL TECNOLOGIA LTDA."),
    ).toBe("UBER");
    expect(forn("DL          *UBERRIDES SAO PAULO     BRA")).toBe("UBER");
    expect(
      forn(
        "PAYPAL *DIGITALOCEA    4029357733    USA",
        "PAYPAL DO BRASIL INSTITUICAO DE PAGAMENTO LTDA",
      ),
    ).toBe("DIGITALOCEAN");
    expect(forn("OPENAI *CHATGPT SUBSCR SAN FRANCISCO USA", "Open AI")).toBe("OPENAI");
    expect(forn("ANTHROPIC* CLAUDE SUB  SAN FRANCISCO USA")).toBe("ANTHROPIC");
  });

  it("tira parcela e id no fim", () => {
    expect(forn("LOJA EXEMPLO PARC 03/10")).toBe("LOJA EXEMPLO");
    expect(forn("LOJA EXEMPLO 03/10")).toBe("LOJA EXEMPLO");
    expect(forn("VINDI  *AUTOLOJAPNEUS PARC03/12")).toBe("VINDI *AUTOLOJAPNEUS");
    expect(forn("PADARIA CENTRAL 123456789")).toBe("PADARIA CENTRAL");
  });

  it("PIX genérico usa o fornecedor; sem fornecedor fica nulo", () => {
    expect(forn("PIX ENVIADO", "TELEFONICA BRASIL S.A.")).toBe("VIVO");
    expect(forn("PIX ENVIADO", "CANAA SERVICOS DE TELECOMUNICACAO LTDA")).toBe(
      "CANAA SERVICOS DE TELECOMUNICACAO",
    );
    expect(forn("PIX ENVIADO", "60.157.753 CLAUDIA CRISTINA CARNEIRO")).toBe(
      "CLAUDIA CRISTINA CARNEIRO",
    );
    expect(forn("PIX ENVIADO")).toBeNull();
    expect(forn("DEB PIX CHAVE")).toBeNull();
    expect(forn("PAG BOLETO IBC", "CAIXA ECONOMICA FEDERAL")).toBeNull();
    expect(forn("PAG BOLETO IBC", "CONDOMNIO DO EDIFICIO YES")).toBe("CONDOMNIO DO EDIFICIO YES");
    expect(forn("DEBITO FUNCEF", "FUNDACAO DOS ECONOMIARIOS FEDERAIS FUNCEF")).toBe("FUNCEF");
  });

  it("extrai o nome de 'Pix enviado para' e 'Pix recebido de'", () => {
    expect(forn("Pix recebido de ANDRE MOURA")).toBe("ANDRE MOURA");
    expect(forn("Pix enviado para 42.200.415 JOSE FRANCISCO VIEIRA DE REZENDE NETO")).toBe(
      "JOSE FRANCISCO VIEIRA DE REZENDE NETO",
    );
  });
});

describe("casos do extrato real", () => {
  it("descrição com barra: 'Transferência enviada|NOME'", () => {
    const r = classificarMovimento(
      mov({
        descricao_original: "Transferência enviada|FRANCISCO LIMA FIGUEIREDO",
        valor_centavos: -3000000,
      }),
    );
    expect(r.tipo).toBe("transferencia_interna");
  });

  it("rolagem de saldo do cartão se anula", () => {
    expect(
      classificarMovimento(mov({ descricao_original: "Saldo em atraso", tipo: "compra" })).tipo,
    ).toBe("transferencia_interna");
    expect(
      classificarMovimento(
        mov({ descricao_original: "Crédito de atraso", tipo: "estorno", valor_centavos: 5 }),
      ).tipo,
    ).toBe("transferencia_interna");
  });
});
