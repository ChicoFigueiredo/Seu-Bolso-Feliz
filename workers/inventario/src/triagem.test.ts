import { describe, expect, it } from "vitest";
import { type AchadoParaTriagem, type OrigemResumo, triar } from "./triagem";

function local(caminho: string): OrigemResumo {
  return {
    origin_type: "local_file",
    local_filepath: caminho,
    filename: caminho.split("/").pop()!,
    gmail_from: null,
    gmail_subject: null,
    gmail_label: null,
    mensagem_ref: null,
  };
}

function gmail(
  de: string,
  assunto: string,
  rotulos = "UNREAD,CATEGORY_UPDATES",
  ref: "eml" | "anexo" = "eml",
  nome = "msg.eml",
): OrigemResumo {
  return {
    origin_type: "gmail",
    local_filepath: null,
    filename: nome,
    gmail_from: de,
    gmail_subject: assunto,
    gmail_label: rotulos,
    mensagem_ref: ref,
  };
}

function achado(
  ext: string,
  origens: OrigemResumo[],
  extra: Partial<AchadoParaTriagem> = {},
): AchadoParaTriagem {
  return {
    content_hash: "h",
    extensao: ext,
    file_size_bytes: 200_000,
    camada: "A",
    origens,
    ...extra,
  };
}

describe("triar: arquivos locais", () => {
  it("ofx é extrato", () => {
    const v = triar(achado("ofx", [local("/mnt/x/qualquer/coisa.ofx")]));
    expect(v).toMatchObject({ estado: "documento", document_type: "extrato" });
  });

  it("pdf em pasta financeira é documento, tipo pelo nome", () => {
    const v = triar(achado("pdf", [local("/mnt/p/OneDrive/Pessoal/Contracheque/2022/c.pdf")]));
    expect(v).toMatchObject({ estado: "documento", document_type: "contracheque" });
    const f = triar(
      achado("pdf", [local("/mnt/p/OneDrive/Pessoal/Contas/2024/Fatura-Nubank.pdf")]),
    );
    expect(f).toMatchObject({ estado: "documento", document_type: "fatura" });
  });

  it("pasta financeira nunca descarta imagem: vira dúvida", () => {
    const v = triar(achado("jpg", [local("/mnt/p/OneDrive/Pessoal/Contas/2024/foto.jpg")]));
    expect(v.estado).toBe("duvida");
  });

  it("nome financeiro fora de pasta financeira", () => {
    const v = triar(achado("pdf", [local("/home/chico/Downloads/Comprovante_2022-10-21.pdf")]));
    expect(v).toMatchObject({ estado: "documento", document_type: "comprovante" });
  });

  it("a origem mais forte vence", () => {
    const v = triar(
      achado("pdf", [
        local("/mnt/bak/Dev/projeto/docs/manual.pdf"),
        local("/mnt/p/OneDrive/Pessoal/IR/2024/informe.pdf"),
      ]),
    );
    expect(v.estado).toBe("documento");
  });

  it("foto pessoal é descartada; captura de tela vira dúvida", () => {
    expect(
      triar(achado("jpg", [local("/mnt/e2/Pessoal/Chico/Fotos/2014/IMG_0001.jpg")])).estado,
    ).toBe("descartado");
    expect(
      triar(achado("png", [local("/mnt/p/OneDrive/Pictures/Screenshots/Screenshot_2024.png")]))
        .estado,
    ).toBe("duvida");
  });

  it("figurinha de WhatsApp e gif são descartados", () => {
    expect(triar(achado("webp", [local("/mnt/x/WhatsApp Stickers/STK-1.webp")])).estado).toBe(
      "descartado",
    );
    expect(triar(achado("gif", [local("/mnt/x/Financeiro/IR/icone.gif")])).estado).toBe(
      "descartado",
    );
  });

  it("código e artefatos de desenvolvimento em B são descartados", () => {
    const v = triar(
      achado("json", [local("/mnt/d/dev/app/node_modules/x/package.json")], { camada: "B" }),
    );
    expect(v.estado).toBe("descartado");
    expect(
      triar(achado("html", [local("/mnt/p2/Dev/site/index.html")], { camada: "B" })).estado,
    ).toBe("descartado");
  });

  it("documentação de programa de IR é descartada, relatório não", () => {
    const pasta = "/mnt/e2/Pessoal/Chico/IR/ImpostoRenda/IRPF2015";
    expect(triar(achado("htm", [local(`${pasta}/Leia-me.htm`)])).estado).toBe("descartado");
    expect(triar(achado("htm", [local(`${pasta}/extrato-corretora.htm`)])).estado).toBe("duvida");
  });

  it("xml: chave de NF-e no nome ou marcas no texto", () => {
    const chave = "3".repeat(44);
    const porNome = triar(achado("xml", [local(`/mnt/x/${chave}.xml`)], { camada: "B" }));
    expect(porNome).toMatchObject({ estado: "documento", document_type: "nota_fiscal" });
    const porTexto = triar(
      achado("xml", [local("/mnt/x/a.xml")], {
        camada: "B",
        texto: '<?xml version="1.0"?><nfeProc><NFe><infNFe Id="NFe1">' + "x".repeat(40),
      }),
    );
    expect(porTexto).toMatchObject({ estado: "documento", document_type: "nota_fiscal" });
    expect(triar(achado("xml", [local("/mnt/x/layout.xml")], { camada: "B" })).estado).toBe(
      "descartado",
    );
  });

  it("print de comprovante Pix lido por OCR vira comprovante", () => {
    const v = triar(
      achado("jpg", [local("/espelho/WhatsApp Images/IMG-20261104-WA0019.jpg")], {
        texto:
          "Pix enviado\nValor\nRS 150,00\nDestino\nFulano de Tal\nCPF ***.123.456-**\nInstituição\nBanco X\nID da transação E2E123",
      }),
    );
    expect(v.estado).toBe("documento");
    expect(v.document_type).toBe("comprovante");
  });

  it("conversa que só cita Pix com valor fica em dúvida, com o tipo sugerido", () => {
    const v = triar(
      achado("jpg", [local("/espelho/WhatsApp Images/IMG-20261104-WA0020.jpg")], {
        texto: "oi, te mandei o pix de R$ 50 ontem, confere aí depois por favor",
      }),
    );
    expect(v.estado).toBe("duvida");
    expect(v.document_type).toBe("comprovante");
  });

  it("texto de boleto promove pdf sem sinal no caminho", () => {
    const v = triar(
      achado("pdf", [local("/mnt/bak/outros/arq1.pdf")], {
        camada: "B",
        texto:
          "Beneficiário X Linha digitável 23790.12345 Vencimento 10/10/2024 Valor do documento 100,00",
      }),
    );
    expect(v).toMatchObject({ estado: "documento", document_type: "boleto" });
  });

  it("texto longo sem termos financeiros descarta pdf sem sinal", () => {
    const v = triar(
      achado("pdf", [local("/mnt/bak/livros/arq1.pdf")], {
        texto: "capítulo sobre geometria. ".repeat(40),
      }),
    );
    expect(v.estado).toBe("descartado");
  });

  it("pdf sem sinal e sem texto fica em dúvida", () => {
    expect(triar(achado("pdf", [local("/mnt/p/OneDrive/Pessoal/Scan0001.pdf")])).estado).toBe(
      "duvida",
    );
  });

  it("pasta financeira vence texto sem termos", () => {
    const v = triar(
      achado("pdf", [local("/mnt/p/OneDrive/Pessoal/Financeiro/a.pdf")], {
        texto: "lorem ipsum ".repeat(50),
      }),
    );
    expect(v.estado).toBe("documento");
  });

  it("achado sem origem fica em dúvida", () => {
    expect(triar(achado("pdf", [])).estado).toBe("duvida");
  });
});

describe("triar: gmail", () => {
  it("newsletter é descartada", () => {
    const v = triar(
      achado("eml", [
        gmail("Post <x@nl.mail.washingtonpost.com>", "Fatura de notícias", "CATEGORY_PROMOTIONS"),
      ]),
    );
    expect(v.estado).toBe("descartado");
  });

  it("promoção de loja é descartada mesmo com emissor conhecido", () => {
    const v = triar(
      achado("eml", [
        gmail("Amazon <x@amazon.com.br>", "Só hoje, ofertas em eBooks", "CATEGORY_PROMOTIONS"),
      ]),
    );
    expect(v.estado).toBe("descartado");
  });

  it("boleto de banco é documento", () => {
    const v = triar(
      achado("eml", [gmail("C6 <nao-responda@c6bank.com.br>", "Boleto emitido para você")]),
    );
    expect(v).toMatchObject({ estado: "documento", document_type: "boleto" });
  });

  it("recibo em inglês de emissor conhecido", () => {
    const v = triar(
      achado("eml", [gmail("DO <billing@digitalocean.com>", "Here's your receipt for team X")]),
    );
    expect(v.estado).toBe("documento");
  });

  it("assunto financeiro de remetente desconhecido é dúvida", () => {
    const v = triar(achado("eml", [gmail("x@empresa-qualquer.com", "Sua fatura chegou")]));
    expect(v.estado).toBe("duvida");
  });

  it("e-mail comum sem sinal é descartado", () => {
    expect(triar(achado("eml", [gmail("amigo@gmail.com", "Churrasco sábado")])).estado).toBe(
      "descartado",
    );
  });

  it("anexo pdf de emissor conhecido é documento; logotipo pequeno é descartado", () => {
    const pdf = gmail("Cemig <x@cemig.com.br>", "Sua conta", "UNREAD", "anexo", "conta.pdf");
    expect(triar(achado("pdf", [pdf])).estado).toBe("documento");
    const logo = gmail("a@b.com", "Oi", "UNREAD", "anexo", "image001.png");
    expect(triar(achado("png", [logo], { file_size_bytes: 4_000 })).estado).toBe("descartado");
  });

  it("anexo pdf de promoção sem nome financeiro é descartado", () => {
    const pdf = gmail(
      "Loja <x@loja.com.br>",
      "Ofertas",
      "CATEGORY_PROMOTIONS",
      "anexo",
      "folheto.pdf",
    );
    expect(triar(achado("pdf", [pdf])).estado).toBe("descartado");
  });

  it("o mesmo conteúdo em e-mail promocional e em pasta financeira é documento", () => {
    const v = triar(
      achado("pdf", [
        gmail("Loja <x@loja.com.br>", "Ofertas", "CATEGORY_PROMOTIONS", "anexo", "a.pdf"),
        local("/mnt/p/OneDrive/Pessoal/Contas/2024/a.pdf"),
      ]),
    );
    expect(v.estado).toBe("documento");
  });

  it("cobrança de dívida vira documento do tipo cobranca", () => {
    const v = triar(
      achado("eml", [gmail("A <x@cobranca.acerto.com.br>", "Aviso de regularização")]),
    );
    expect(v).toMatchObject({ estado: "documento", document_type: "cobranca" });
  });
});
