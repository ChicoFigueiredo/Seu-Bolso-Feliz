//! Arquivos fictícios para o modo demonstração: a prévia precisa de algo para mostrar.
//!
//! Tudo é gerado aqui, sem dependência: um PDF de 2 ou 3 páginas com texto de boleto ou
//! fatura fictícios, um PNG de comprovante de PIX fictício (fonte bitmap 5x7), um `.eml`
//! com um PDF anexado, um CSV de extrato e um XML de nota fiscal. Nenhum dado é real.

use sha2::{Digest, Sha256};

use crate::formato;

/// Os dados de um documento fictício.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDoDocumento {
    pub nome: String,
    pub fornecedor: Option<String>,
    /// `boleto`, `fatura_cartao`, `extrato`…
    pub tipo: String,
    pub data: Option<String>,
    pub valor: Option<i64>,
}

/// O sha256 em hex minúsculo (o `content_hash` do inventário).
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ----------------------------------------------------------------------------- PDF

/// Texto para o PDF (Helvetica, WinAnsi): Latin-1, com os parênteses escapados.
fn texto_pdf(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len());
    for c in s.chars() {
        let c = match c {
            '−' | '–' | '—' => '-',
            '“' | '”' => '"',
            '‘' | '’' => '\'',
            '…' => '.',
            c => c,
        };
        match c {
            '(' | ')' | '\\' => {
                v.push(b'\\');
                v.push(c as u8);
            }
            c if (c as u32) < 256 => v.push(c as u32 as u8),
            _ => v.push(b'?'),
        }
    }
    v
}

/// Um elemento de uma página: texto ou retângulo cheio (em cinza ou cor).
#[derive(Debug, Clone)]
enum Peca {
    Texto {
        x: f32,
        y: f32,
        tamanho: f32,
        negrito: bool,
        texto: String,
    },
    Caixa {
        x: f32,
        y: f32,
        l: f32,
        a: f32,
        cor: (f32, f32, f32),
    },
}

fn conteudo_da_pagina(pecas: &[Peca]) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::new();
    for p in pecas {
        match p {
            Peca::Caixa { x, y, l, a, cor } => {
                s.extend_from_slice(
                    format!(
                        "{:.3} {:.3} {:.3} rg {x:.1} {y:.1} {l:.1} {a:.1} re f\n",
                        cor.0, cor.1, cor.2
                    )
                    .as_bytes(),
                );
            }
            Peca::Texto {
                x,
                y,
                tamanho,
                negrito,
                texto,
            } => {
                let fonte = if *negrito { "F2" } else { "F1" };
                s.extend_from_slice(
                    format!("0.17 0.2 0.19 rg BT /{fonte} {tamanho:.1} Tf {x:.1} {y:.1} Td (")
                        .as_bytes(),
                );
                s.extend_from_slice(&texto_pdf(texto));
                s.extend_from_slice(b") Tj ET\n");
            }
        }
    }
    s
}

fn pdf_de_paginas(paginas: &[Vec<Peca>]) -> Vec<u8> {
    let n = paginas.len();
    // Objetos: 1 catálogo, 2 páginas, 3 Helvetica, 4 Helvetica-Bold, depois (página,
    // conteúdo) para cada página.
    let mut objetos: Vec<Vec<u8>> = Vec::new();
    objetos.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let filhos: Vec<String> = (0..n).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
    objetos
        .push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", filhos.join(" ")).into_bytes());
    objetos.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    objetos.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    for (i, pecas) in paginas.iter().enumerate() {
        objetos.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                6 + 2 * i
            )
            .into_bytes(),
        );
        let c = conteudo_da_pagina(pecas);
        let mut o = format!("<< /Length {} >>\nstream\n", c.len()).into_bytes();
        o.extend_from_slice(&c);
        o.extend_from_slice(b"\nendstream");
        objetos.push(o);
    }
    let mut saida = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut posicoes = Vec::new();
    for (i, o) in objetos.iter().enumerate() {
        posicoes.push(saida.len());
        saida.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        saida.extend_from_slice(o);
        saida.extend_from_slice(b"\nendobj\n");
    }
    let xref = saida.len();
    saida.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objetos.len() + 1).as_bytes(),
    );
    for p in posicoes {
        saida.extend_from_slice(format!("{p:010} 00000 n \n").as_bytes());
    }
    saida.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objetos.len() + 1
        )
        .as_bytes(),
    );
    saida
}

fn t(x: f32, y: f32, tamanho: f32, texto: impl Into<String>) -> Peca {
    Peca::Texto {
        x,
        y,
        tamanho,
        negrito: false,
        texto: texto.into(),
    }
}

fn tn(x: f32, y: f32, tamanho: f32, texto: impl Into<String>) -> Peca {
    Peca::Texto {
        x,
        y,
        tamanho,
        negrito: true,
        texto: texto.into(),
    }
}

fn caixa(x: f32, y: f32, l: f32, a: f32, cor: (f32, f32, f32)) -> Peca {
    Peca::Caixa { x, y, l, a, cor }
}

const SALVIA: (f32, f32, f32) = (0.24, 0.42, 0.35);
const CINZA_CLARO: (f32, f32, f32) = (0.93, 0.94, 0.93);
const CINZA: (f32, f32, f32) = (0.55, 0.58, 0.57);
const PRETO: (f32, f32, f32) = (0.1, 0.1, 0.1);

fn rotulo_do_tipo(tipo: &str) -> &'static str {
    match tipo {
        "boleto" => "BOLETO DE COBRANÇA",
        "fatura_cartao" => "FATURA DO CARTÃO",
        "extrato" => "EXTRATO DE CONTA",
        "comprovante" => "COMPROVANTE",
        "nota_fiscal" => "NOTA FISCAL",
        "contrato" => "CONTRATO",
        _ => "DOCUMENTO",
    }
}

/// Um código de barras de mentira: barras de larguras que vêm do nome.
fn barras(semente: &str, y: f32) -> Vec<Peca> {
    let mut v = Vec::new();
    let mut x = 50.0;
    let mut h: u32 = 2_166_136_261;
    while x < 540.0 {
        for b in semente.bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
        }
        let l = 1.0 + (h % 3) as f32;
        v.push(caixa(x, y, l, 46.0, PRETO));
        x += l + 1.0 + ((h >> 3) % 3) as f32;
    }
    v
}

/// Um PDF fictício de 2 páginas (3 numa fatura) com o texto de um boleto, fatura ou
/// extrato de mentira.
pub fn pdf_do_documento(d: &DadosDoDocumento) -> Vec<u8> {
    let fornecedor = d
        .fornecedor
        .clone()
        .unwrap_or_else(|| "Empresa Exemplo".to_owned());
    let data = d
        .data
        .as_deref()
        .map(formato::data)
        .unwrap_or_else(|| "—".to_owned());
    let valor = d
        .valor
        .map(|v| formato::dinheiro(v.abs()))
        .unwrap_or_else(|| "—".to_owned());
    let titulo = rotulo_do_tipo(&d.tipo);
    let mut p1 = vec![
        caixa(0.0, 772.0, 595.0, 70.0, SALVIA),
        Peca::Texto {
            x: 50.0,
            y: 806.0,
            tamanho: 20.0,
            negrito: true,
            texto: fornecedor.to_uppercase(),
        },
        t(
            50.0,
            786.0,
            10.0,
            "CNPJ 00.000.000/0001-00 · documento fictício de demonstração",
        ),
        tn(50.0, 735.0, 16.0, titulo),
        t(50.0, 715.0, 10.0, format!("Arquivo: {}", d.nome)),
        caixa(50.0, 600.0, 495.0, 95.0, CINZA_CLARO),
        t(66.0, 672.0, 10.0, "Pagador"),
        tn(66.0, 656.0, 12.0, "Cliente Exemplo da Silva"),
        t(66.0, 636.0, 10.0, "Rua Fictícia, 123 · Cidade Exemplo"),
        t(330.0, 672.0, 10.0, "Vencimento"),
        tn(330.0, 652.0, 16.0, data.clone()),
        t(330.0, 630.0, 10.0, "Valor do documento"),
        tn(330.0, 610.0, 16.0, valor.clone()),
        tn(50.0, 560.0, 11.0, "Discriminação"),
    ];
    let linhas = match d.tipo.as_str() {
        "fatura_cartao" => vec![
            ("Compras nacionais", 0.62),
            ("Assinaturas e serviços", 0.18),
            ("Parcelamentos", 0.15),
            ("Encargos", 0.05),
        ],
        "extrato" => vec![
            ("Saldo anterior", 0.0),
            ("Créditos no período", 0.0),
            ("Débitos no período", 0.0),
            ("Saldo final", 0.0),
        ],
        _ => vec![
            ("Consumo do período", 0.81),
            ("Taxa de serviço", 0.12),
            ("Tributos", 0.07),
        ],
    };
    let mut y = 538.0;
    for (rotulo, fracao) in &linhas {
        p1.push(t(60.0, y, 10.5, *rotulo));
        if let (Some(v), true) = (d.valor, *fracao > 0.0) {
            let parte = (v.abs() as f64 * fracao).round() as i64;
            p1.push(t(440.0, y, 10.5, formato::dinheiro(parte)));
        }
        p1.push(caixa(60.0, y - 6.0, 470.0, 0.6, CINZA));
        y -= 22.0;
    }
    p1.push(tn(60.0, y - 6.0, 11.0, "Total"));
    p1.push(tn(440.0, y - 6.0, 11.0, valor.clone()));
    p1.push(t(50.0, 190.0, 9.0, "Linha digitável (fictícia)"));
    p1.push(tn(
        50.0,
        172.0,
        12.0,
        "00190.00009 01234.567891 23456.789012 3 00000000000000",
    ));
    p1.extend(barras(&d.nome, 110.0));
    p1.push(t(
        50.0,
        80.0,
        8.0,
        "Este documento foi gerado para a demonstração do Seu Bolso Feliz. Não tem valor.",
    ));
    p1.push(t(500.0, 40.0, 9.0, "pág. 1"));

    let mut p2 = vec![
        caixa(0.0, 802.0, 595.0, 40.0, SALVIA),
        tn(50.0, 816.0, 13.0, format!("{} · detalhamento", fornecedor)),
        tn(50.0, 760.0, 13.0, "Histórico dos últimos meses"),
    ];
    let mut y = 730.0;
    let base = d.valor.unwrap_or(-15000).abs();
    for (i, mes) in ["mai", "jun", "jul", "ago", "set", "out"]
        .iter()
        .enumerate()
    {
        let v = base - (5 - i as i64) * base / 40;
        let l = 380.0 * v as f32 / (base as f32 * 1.1);
        p2.push(t(50.0, y, 10.0, format!("{mes}/2026")));
        p2.push(caixa(110.0, y - 2.0, l.max(4.0), 11.0, SALVIA));
        p2.push(t(110.0 + l + 8.0, y, 9.5, formato::dinheiro(v)));
        y -= 24.0;
    }
    p2.push(tn(50.0, 540.0, 13.0, "Informações importantes"));
    let avisos = [
        "Pague até o vencimento para evitar multa de 2% e juros de 1% ao mês.",
        "Dúvidas: central de atendimento fictícia 0800 000 0000.",
        "Débito automático: cadastre no seu banco com o código do cliente.",
        "Guarde este documento por cinco anos.",
    ];
    let mut y = 515.0;
    for a in avisos {
        p2.push(t(60.0, y, 10.0, format!("• {a}")));
        y -= 18.0;
    }
    p2.push(t(500.0, 40.0, 9.0, "pág. 2"));
    let mut paginas = vec![p1, p2];
    if d.tipo == "fatura_cartao" {
        let mut p3 = vec![
            caixa(0.0, 802.0, 595.0, 40.0, SALVIA),
            tn(50.0, 816.0, 13.0, "Lançamentos do cartão"),
        ];
        let mut y = 760.0;
        for (i, (loja, v)) in [
            ("STREAMING ZETA", 3990),
            ("MERCADO DELTA LJ 12", 41230),
            ("LIVRARIA OMICRON", 8990),
            ("MERCADO DELTA LJ 12", 28760),
            ("FARMACIA EPSILON", 6420),
        ]
        .iter()
        .enumerate()
        {
            p3.push(t(50.0, y, 10.0, format!("{:02}/10", 3 + i * 5)));
            p3.push(t(110.0, y, 10.0, *loja));
            p3.push(t(440.0, y, 10.0, formato::dinheiro(*v)));
            y -= 20.0;
        }
        p3.push(t(500.0, 40.0, 9.0, "pág. 3"));
        paginas.push(p3);
    }
    pdf_de_paginas(&paginas)
}

// ----------------------------------------------------------------------------- PNG

fn crc32(dados: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in dados {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let m = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & m);
        }
    }
    !crc
}

fn adler32(dados: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in dados {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn bloco(saida: &mut Vec<u8>, tipo: &[u8; 4], dados: &[u8]) {
    saida.extend_from_slice(&(dados.len() as u32).to_be_bytes());
    let mut c = tipo.to_vec();
    c.extend_from_slice(dados);
    saida.extend_from_slice(&c);
    saida.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// RGBA em PNG sem compressão (blocos "stored" do zlib).
pub fn png(largura: usize, altura: usize, rgba: &[u8]) -> Vec<u8> {
    let mut cru = Vec::with_capacity((largura * 4 + 1) * altura);
    for linha in rgba.chunks(largura * 4).take(altura) {
        cru.push(0);
        cru.extend_from_slice(linha);
    }
    let mut z = vec![0x78, 0x01];
    let mut pedacos = cru.chunks(65_535).peekable();
    while let Some(p) = pedacos.next() {
        z.push(u8::from(pedacos.peek().is_none()));
        let n = p.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(p);
    }
    z.extend_from_slice(&adler32(&cru).to_be_bytes());
    let mut saida = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(largura as u32).to_be_bytes());
    ihdr.extend_from_slice(&(altura as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    bloco(&mut saida, b"IHDR", &ihdr);
    bloco(&mut saida, b"IDAT", &z);
    bloco(&mut saida, b"IEND", &[]);
    saida
}

/// Glifos 5x7 (linhas de 5 bits, o bit 4 à esquerda) para maiúsculas, dígitos e alguns
/// sinais. O resto vira espaço.
fn glifo(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '$' => [4, 15, 20, 14, 5, 30, 4],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '*' => [0, 4, 21, 14, 21, 4, 0],
        _ => [0; 7],
    }
}

fn sem_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'ã' | 'â' | 'Á' | 'À' | 'Ã' | 'Â' => 'A',
        'é' | 'ê' | 'É' | 'Ê' => 'E',
        'í' | 'Í' => 'I',
        'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
        'ú' | 'Ú' => 'U',
        'ç' | 'Ç' => 'C',
        c => c.to_ascii_uppercase(),
    }
}

struct Tela {
    l: usize,
    a: usize,
    px: Vec<u8>,
}

impl Tela {
    fn nova(l: usize, a: usize, fundo: [u8; 3]) -> Self {
        let mut px = Vec::with_capacity(l * a * 4);
        for _ in 0..l * a {
            px.extend_from_slice(&[fundo[0], fundo[1], fundo[2], 255]);
        }
        Self { l, a, px }
    }

    fn caixa(&mut self, x: usize, y: usize, l: usize, a: usize, cor: [u8; 3]) {
        for yy in y..(y + a).min(self.a) {
            for xx in x..(x + l).min(self.l) {
                let i = (yy * self.l + xx) * 4;
                self.px[i..i + 3].copy_from_slice(&cor);
            }
        }
    }

    fn texto(&mut self, x: usize, y: usize, escala: usize, cor: [u8; 3], texto: &str) {
        let mut cx = x;
        for c in texto.chars().map(sem_acento) {
            for (linha, bits) in glifo(c).iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.caixa(cx + col * escala, y + linha * escala, escala, escala, cor);
                    }
                }
            }
            cx += 6 * escala;
        }
    }

    fn circulo(&mut self, cx: f32, cy: f32, r: f32, cor: [u8; 3]) {
        for y in 0..self.a {
            for x in 0..self.l {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                if dx * dx + dy * dy <= r * r {
                    let i = (y * self.l + x) * 4;
                    self.px[i..i + 3].copy_from_slice(&cor);
                }
            }
        }
    }
}

/// O PNG de um comprovante de PIX fictício (540 x 960).
pub fn png_do_comprovante(valor_centavos: i64, data: &str) -> Vec<u8> {
    let verde = [0x2e, 0x7d, 0x5b];
    let tinta = [0x2b, 0x34, 0x30];
    let suave = [0x6b, 0x76, 0x71];
    let mut t = Tela::nova(540, 960, [0xfb, 0xfc, 0xfb]);
    t.caixa(0, 0, 540, 150, verde);
    t.texto(40, 50, 5, [255, 255, 255], "COMPROVANTE PIX");
    t.texto(
        40,
        105,
        2,
        [0xd8, 0xee, 0xe3],
        "BANCO FICTICIO S.A. - DEMONSTRACAO",
    );
    t.circulo(270.0, 250.0, 56.0, verde);
    // O "check": duas barras brancas.
    for k in 0..34 {
        t.caixa(238 + k, 250 + k / 2, 8, 8, [255, 255, 255]);
    }
    for k in 0..52 {
        t.caixa(266 + k, 266 - k * 2 / 3, 8, 8, [255, 255, 255]);
    }
    t.texto(40, 350, 3, suave, "VALOR");
    let valor = formato::dinheiro(valor_centavos.abs());
    t.texto(40, 385, 7, tinta, &valor);
    let linhas = [
        ("DATA", data.to_owned()),
        ("DE", "CLIENTE EXEMPLO DA SILVA".to_owned()),
        ("PARA", "LOJA FICTICIA LTDA".to_owned()),
        ("CHAVE", "CONTATO*EXEMPLO.EXAMPLE".to_owned()),
        ("ID", "E0000000020261112FICTICIO".to_owned()),
    ];
    let mut y = 500;
    for (r, v) in linhas {
        t.caixa(40, y - 14, 460, 2, [0xe3, 0xe8, 0xe5]);
        t.texto(40, y, 3, suave, r);
        t.texto(40, y + 32, 3, tinta, &v);
        y += 86;
    }
    t.texto(40, 920, 2, suave, "DOCUMENTO FICTICIO - SEM VALOR");
    png(t.l, t.a, &t.px)
}

// ----------------------------------------------------------------------------- texto

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 em linhas de 76 (anexo de e-mail).
pub fn base64(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 4 / 3 + bytes.len() / 57 * 2 + 4);
    for (i, b) in bytes.chunks(3).enumerate() {
        if i > 0 && i % 19 == 0 {
            s.push_str("\r\n");
        }
        let n = (u32::from(b[0]) << 16)
            | (u32::from(*b.get(1).unwrap_or(&0)) << 8)
            | u32::from(*b.get(2).unwrap_or(&0));
        s.push(BASE64[(n >> 18) as usize & 63] as char);
        s.push(BASE64[(n >> 12) as usize & 63] as char);
        s.push(if b.len() > 1 {
            BASE64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if b.len() > 2 {
            BASE64[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

/// Um e-mail fictício (`.eml`) com o PDF anexado.
pub fn eml(de: &str, assunto: &str, corpo: &str, nome_do_anexo: &str, anexo: &[u8]) -> Vec<u8> {
    format!(
        "From: {de}\r\nTo: cliente@exemplo.example\r\nSubject: {assunto}\r\nDate: Wed, 11 Nov 2026 09:14:00 -0300\r\nMessage-ID: <demonstracao-1@exemplo.example>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"sbf-demonstracao\"\r\n\r\n--sbf-demonstracao\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{corpo}\r\n\r\n--sbf-demonstracao\r\nContent-Type: application/pdf; name=\"{nome_do_anexo}\"\r\nContent-Disposition: attachment; filename=\"{nome_do_anexo}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--sbf-demonstracao--\r\n",
        base64(anexo)
    )
    .into_bytes()
}

/// O CSV de um extrato fictício.
pub fn csv_do_extrato() -> Vec<u8> {
    let mut s = String::from("data;descricao;valor;saldo\n");
    let linhas = [
        ("2026-10-01", "Saldo anterior", 0, 412_350),
        ("2026-10-01", "Tarifa de pacote", -3_990, 408_360),
        (
            "2026-10-02",
            "Pagamento da fatura do Cartão Beta",
            -82_970,
            325_390,
        ),
        (
            "2026-10-04",
            "Transferência para a poupança",
            -100_000,
            225_390,
        ),
        ("2026-10-05", "PAG CONTA ENERGIA EXEMPLO", -28_710, 196_680),
        ("2026-10-13", "Mensalidade Escola Gama", -189_000, 7_680),
        ("2026-10-30", "Salário · Empresa Exemplo", 850_000, 857_680),
    ];
    for (d, desc, v, saldo) in linhas {
        s.push_str(&format!(
            "{d};{desc};{:.2};{:.2}\n",
            v as f64 / 100.0,
            saldo as f64 / 100.0
        ));
    }
    s.into_bytes()
}

/// O XML de uma nota fiscal fictícia.
pub fn xml_da_nota() -> Vec<u8> {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nfeProc versao=\"4.00\">\n  <NFe>\n    <infNFe Id=\"NFe35261100000000000000550010000000011000000011\">\n      <ide><nNF>1</nNF><dhEmi>2026-11-12T10:20:00-03:00</dhEmi></ide>\n      <emit><CNPJ>00000000000100</CNPJ><xNome>LOJA FICTICIA LTDA</xNome></emit>\n      <dest><xNome>CLIENTE EXEMPLO DA SILVA</xNome></dest>\n      <det nItem=\"1\"><prod><xProd>Produto de demonstração</xProd><vProd>125.00</vProd></prod></det>\n      <total><ICMSTot><vNF>125.00</vNF></ICMSTot></total>\n    </infNFe>\n  </NFe>\n</nfeProc>\n"
        .as_bytes()
        .to_vec()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dados() -> DadosDoDocumento {
        DadosDoDocumento {
            nome: "fatura-beta-out.pdf".into(),
            fornecedor: Some("Banco Beta".into()),
            tipo: "fatura_cartao".into(),
            data: Some("2026-10-25".into()),
            valor: Some(-82970),
        }
    }

    #[test]
    fn pdf_tem_cabecalho_paginas_e_xref() {
        let pdf = pdf_do_documento(&dados());
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let texto = String::from_utf8_lossy(&pdf);
        assert!(texto.contains("/Count 3"), "fatura tem 3 páginas");
        // A posição do xref aponta para "xref".
        let pos: usize = texto
            .rsplit("startxref\n")
            .next()
            .and_then(|r| r.lines().next())
            .and_then(|n| n.parse().ok())
            .expect("startxref");
        assert_eq!(&pdf[pos..pos + 4], b"xref");
        let boleto = DadosDoDocumento {
            tipo: "boleto".into(),
            ..dados()
        };
        assert!(String::from_utf8_lossy(&pdf_do_documento(&boleto)).contains("/Count 2"));
    }

    #[test]
    fn pdf_e_deterministico() {
        assert_eq!(pdf_do_documento(&dados()), pdf_do_documento(&dados()));
    }

    #[test]
    fn png_tem_assinatura_e_tamanho() {
        let p = png_do_comprovante(12500, "12/11/2026 14:32");
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(u32::from_be_bytes([p[16], p[17], p[18], p[19]]), 540);
        assert_eq!(u32::from_be_bytes([p[20], p[21], p[22], p[23]]), 960);
    }

    #[test]
    fn base64_confere() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn sha256_conhecido() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
