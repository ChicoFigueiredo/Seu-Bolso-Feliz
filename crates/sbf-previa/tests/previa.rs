//! A prévia de cada tipo, com arquivos fictícios gerados pelo teste.

mod comum;

use comum::*;
use sbf_previa::{Conteudo, ErroDaPrevia, Familia, Pedido, familia, miniatura};
use std::io::Write;
use std::time::Instant;

fn paginas(conteudo: &Conteudo) -> &sbf_previa::Paginas {
    match conteudo {
        Conteudo::Paginas(p) => p,
        outro => panic!("esperava páginas: {outro:?}"),
    }
}

#[test]
fn a_familia_vem_da_extensao() {
    assert_eq!(familia("PDF"), Familia::Pdf);
    assert_eq!(familia(".jpeg"), Familia::Imagem);
    assert_eq!(familia("heic"), Familia::Heic);
    assert_eq!(familia("eml"), Familia::Email);
    assert_eq!(familia("ofx"), Familia::Texto);
    assert_eq!(familia("xlsx"), Familia::Office);
    assert_eq!(familia("zip"), Familia::Zip);
    assert_eq!(familia("exe"), Familia::Outro);
}

#[test]
fn pdf_de_tres_paginas_conta_e_desenha_cada_pagina() {
    let pasta = pasta();
    let caminho = pasta.path().join("fatura.pdf");
    std::fs::write(&caminho, pdf_ficticio(3)).expect("gravar");

    let inicio = Instant::now();
    let conteudo = abrir(&caminho, vec![]);
    let abrir_ms = inicio.elapsed().as_millis();
    let p = paginas(&conteudo);
    assert_eq!(p.total(), 3);

    let inicio = Instant::now();
    let pagina2 = p.desenhar(2, 600).expect("desenhar");
    let desenhar_ms = inicio.elapsed().as_millis();
    eprintln!("MEDIDA pdf: abrir {abrir_ms} ms, página 600 px {desenhar_ms} ms");
    assert_eq!(pagina2.largura, 600);
    // A4: 842/595 da largura.
    assert!((845..=852).contains(&pagina2.altura), "{}", pagina2.altura);
    assert_eq!(pagina2.rgba.len(), (600 * pagina2.altura * 4) as usize);
    // Página 2: retângulo preto de 200 pt no alto; a página 1 tem 100 pt.
    let y = pagina2.altura * 150 / 842;
    assert!(
        escuro(pixel(&pagina2, 300, y)),
        "página 2 devia ser preta em y={y}"
    );
    let pagina1 = p.desenhar(1, 600).expect("desenhar");
    assert!(
        !escuro(pixel(&pagina1, 300, y)),
        "página 1 devia ser branca em y={y}"
    );
    assert_ne!(pagina1, pagina2);

    assert!(matches!(p.desenhar(4, 600), Err(ErroDaPrevia::Ilegivel(_))));
    assert!(matches!(p.desenhar(0, 600), Err(ErroDaPrevia::Ilegivel(_))));

    let mini = miniatura(&conteudo, 220)
        .expect("miniatura")
        .expect("imagem");
    assert_eq!(mini.largura, 220);
}

#[test]
fn pdf_protegido_abre_com_a_candidata_certa_e_sem_ela_fica_protegido() {
    let pasta = pasta();
    let caminho = cifrar(pasta.path(), "protegido", &pdf_ficticio(3), SENHA);

    match abrir(&caminho, vec![]) {
        Conteudo::Protegido { motivo } => assert!(motivo.contains("nenhuma regra"), "{motivo}"),
        outro => panic!("{outro:?}"),
    }
    match abrir(&caminho, vec![candidata("errada-1"), candidata("errada-2")]) {
        Conteudo::Protegido { motivo } => assert!(motivo.contains("2 senhas"), "{motivo}"),
        outro => panic!("{outro:?}"),
    }
    assert!(!format!("{:?}", abrir(&caminho, vec![candidata("x")])).contains(SENHA));

    let inicio = Instant::now();
    let conteudo = abrir(&caminho, vec![candidata("errada"), candidata(SENHA)]);
    let p = paginas(&conteudo);
    assert_eq!(p.total(), 3);
    let pagina3 = p.desenhar(3, 400).expect("desenhar");
    eprintln!(
        "MEDIDA pdf protegido: decifrar e desenhar {} ms",
        inicio.elapsed().as_millis()
    );
    let y = pagina3.altura * 250 / 842;
    assert!(escuro(pixel(&pagina3, 200, y)));
}

#[test]
fn imagens_png_e_jpeg_viram_rgba_e_as_grandes_sao_reduzidas() {
    let pasta = pasta();
    let png = pasta.path().join("foto.png");
    gravar_imagem(&png, 300, 200);
    let conteudo = abrir(&png, vec![]);
    let Conteudo::Imagem(i) = &conteudo else {
        panic!("{conteudo:?}")
    };
    assert_eq!((i.largura, i.altura), (300, 200));
    assert_eq!(pixel(i, 10, 10), [220, 20, 20, 255]);
    assert_eq!(pixel(i, 10, 190), [20, 20, 220, 255]);
    let mini = miniatura(&conteudo, 150).expect("mini").expect("imagem");
    assert_eq!((mini.largura, mini.altura), (150, 100));

    // JPEG grande: reduzido para 2400 no lado maior.
    let jpg = pasta.path().join("recibo.jpg");
    gravar_imagem(&jpg, 4800, 1200);
    let inicio = Instant::now();
    let conteudo = abrir(&jpg, vec![]);
    eprintln!(
        "MEDIDA imagem: jpeg 4800x1200 aberto e reduzido em {} ms",
        inicio.elapsed().as_millis()
    );
    let Conteudo::Imagem(i) = &conteudo else {
        panic!("{conteudo:?}")
    };
    assert_eq!((i.largura, i.altura), (2400, 600));
    let p = pixel(i, 100, 50);
    assert!(p[0] > 180 && p[2] < 60, "{p:?}");

    // A extensão do achado manda: um PNG com nome .bin abre como imagem.
    let bin = pasta.path().join("sem-nome.bin");
    std::fs::copy(&png, &bin).expect("copiar");
    let conteudo = sbf_previa::abrir(Pedido {
        arquivo: &bin,
        extensao: "png",
        candidatas: vec![],
    })
    .expect("abrir");
    assert!(matches!(conteudo, Conteudo::Imagem(_)));
}

#[test]
fn imagem_acima_de_64_megapixels_e_recusada_antes_de_decodificar() {
    let pasta = pasta();
    // Um PNG de 9000 x 8000 (72 MP) em escala de cinza de 1 bit: pequeno no disco.
    let caminho = pasta.path().join("enorme.png");
    let arquivo = std::fs::File::create(&caminho).expect("criar");
    let mut codificador = png_encoder(arquivo, 9000, 8000);
    let linha = vec![0_u8; 9000usize.div_ceil(8)];
    let mut escritor = codificador.stream_writer().expect("stream");
    for _ in 0..8000 {
        escritor.write_all(&linha).expect("linha");
    }
    escritor.finish().expect("fim");
    drop(codificador);
    let erro = sbf_previa::abrir(Pedido {
        arquivo: &caminho,
        extensao: "png",
        candidatas: vec![],
    })
    .expect_err("devia recusar");
    assert!(matches!(erro, ErroDaPrevia::GrandeDemais), "{erro:?}");
}

fn png_encoder(
    arquivo: std::fs::File,
    largura: u32,
    altura: u32,
) -> png::Writer<std::io::BufWriter<std::fs::File>> {
    let mut e = png::Encoder::new(std::io::BufWriter::new(arquivo), largura, altura);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::One);
    e.write_header().expect("cabeçalho")
}

#[test]
fn arquivo_acima_de_50_mib_e_recusado_sem_ler() {
    let pasta = pasta();
    let caminho = pasta.path().join("grande.pdf");
    let arquivo = std::fs::File::create(&caminho).expect("criar");
    arquivo
        .set_len(sbf_previa::TAMANHO_MAXIMO + 1)
        .expect("esparso");
    let erro = sbf_previa::abrir(Pedido {
        arquivo: &caminho,
        extensao: "pdf",
        candidatas: vec![],
    })
    .expect_err("devia recusar");
    assert!(matches!(erro, ErroDaPrevia::GrandeDemais), "{erro:?}");
    let erro = sbf_previa::abrir(Pedido {
        arquivo: &pasta.path().join("nao-existe.pdf"),
        extensao: "pdf",
        candidatas: vec![],
    })
    .expect_err("devia faltar");
    assert!(matches!(erro, ErroDaPrevia::SemArquivo), "{erro:?}");
}

const EML: &str = "From: Banco Ficticio <avisos@banco-ficticio.example>\r\n\
To: Cliente Exemplo <cliente@exemplo.example>\r\n\
Subject: =?UTF-8?Q?Sua_fatura_est=C3=A1_dispon=C3=ADvel?=\r\n\
Date: Thu, 12 Nov 2026 09:30:00 -0300\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"LIMITE\"\r\n\
\r\n\
--LIMITE\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Ol&aacute;, sua fatura de <b>R$ 123,45</b> chegou.</p></body></html>\r\n\
--LIMITE\r\n\
Content-Type: application/pdf; name=\"fatura.pdf\"\r\n\
Content-Disposition: attachment; filename=\"fatura.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
SGVsbG8sIGFuZXhvIGZpY3RpY2lvIQ==\r\n\
--LIMITE--\r\n";

#[test]
fn email_mostra_cabecalho_corpo_em_texto_e_anexos_com_sha256() {
    let pasta = pasta();
    let caminho = pasta.path().join("aviso.eml");
    std::fs::write(&caminho, EML).expect("gravar");
    let Conteudo::Email(e) = abrir(&caminho, vec![]) else {
        panic!("esperava e-mail")
    };
    assert_eq!(e.de, "Banco Ficticio <avisos@banco-ficticio.example>");
    assert_eq!(e.para, "Cliente Exemplo <cliente@exemplo.example>");
    assert_eq!(e.assunto, "Sua fatura está disponível");
    assert_eq!(e.data, "Thu, 12 Nov 2026 09:30:00 -0300");
    assert!(
        e.corpo.contains("Olá, sua fatura de R$ 123,45 chegou."),
        "{}",
        e.corpo
    );
    assert!(!e.corpo.contains("<b>"));
    assert_eq!(e.anexos.len(), 1);
    let anexo = &e.anexos[0];
    assert_eq!(anexo.nome, "fatura.pdf");
    let conteudo = b"Hello, anexo ficticio!";
    assert_eq!(anexo.bytes, conteudo.len() as u64);
    // sha256 de "Hello, anexo ficticio!" (sha256sum).
    let esperado = std::process::Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().expect("stdin").write_all(conteudo)?;
            c.wait_with_output()
        })
        .expect("sha256sum");
    let esperado = String::from_utf8_lossy(&esperado.stdout);
    assert_eq!(anexo.sha256.as_deref(), esperado.split_whitespace().next());
    assert!(miniatura(&Conteudo::Email(e), 100).expect("mini").is_none());

    let msg = pasta.path().join("outlook.msg");
    std::fs::write(&msg, b"\xD0\xCF\x11\xE0").expect("gravar");
    assert!(matches!(abrir(&msg, vec![]), Conteudo::SemPrevia { .. }));
}

#[test]
fn texto_csv_utf8_latin1_e_html() {
    let pasta = pasta();
    let csv = pasta.path().join("extrato.csv");
    std::fs::write(
        &csv,
        "\u{feff}data;descrição;valor\n05/11/2026;MERCADO;-12,34\n",
    )
    .expect("gravar");
    let Conteudo::Texto(t) = abrir(&csv, vec![]) else {
        panic!("texto")
    };
    assert_eq!(t, "data;descrição;valor\n05/11/2026;MERCADO;-12,34\n");

    let ofx = pasta.path().join("extrato.ofx");
    std::fs::write(&ofx, b"<MEMO>PAGAMENTO CART\xC3O\n").expect("gravar");
    let Conteudo::Texto(t) = abrir(&ofx, vec![]) else {
        panic!("texto")
    };
    assert_eq!(t, "<MEMO>PAGAMENTO CARTÃO\n");

    let html = pasta.path().join("aviso.html");
    std::fs::write(
        &html,
        "<html><body><h1>Boleto</h1><p>Valor: R$ 9,99</p></body></html>",
    )
    .expect("gravar");
    let Conteudo::Texto(t) = abrir(&html, vec![]) else {
        panic!("texto")
    };
    assert!(
        t.contains("Boleto") && t.contains("Valor: R$ 9,99") && !t.contains("<p>"),
        "{t}"
    );

    // Só o começo de um texto grande.
    let grande = pasta.path().join("grande.txt");
    std::fs::write(&grande, "á".repeat(300 * 1024)).expect("gravar");
    let Conteudo::Texto(t) = abrir(&grande, vec![]) else {
        panic!("texto")
    };
    assert!(t.len() < sbf_previa::TEXTO_MAXIMO + 200);
    assert!(t.starts_with("áá") && t.ends_with("(a prévia mostra só o começo do arquivo)"));
}

#[test]
fn zip_lista_as_entradas_sem_extrair() {
    let pasta = pasta();
    let caminho = pasta.path().join("faturas.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&caminho).expect("criar"));
    let opcoes =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("2026-10/fatura.pdf", opcoes)
        .expect("entrada");
    zip.write_all(&[7_u8; 1234]).expect("bytes");
    zip.start_file("leia-me.txt", opcoes).expect("entrada");
    zip.write_all(b"oi").expect("bytes");
    zip.finish().expect("fim");
    let antes: Vec<_> = std::fs::read_dir(pasta.path()).expect("ler").collect();
    let Conteudo::ListaDoZip(lista) = abrir(&caminho, vec![]) else {
        panic!("zip")
    };
    assert_eq!(lista.len(), 2);
    assert_eq!(lista[0].nome, "2026-10/fatura.pdf");
    assert_eq!(lista[0].bytes, 1234);
    assert_eq!(lista[1].nome, "leia-me.txt");
    assert_eq!(lista[1].bytes, 2);
    let depois: Vec<_> = std::fs::read_dir(pasta.path()).expect("ler").collect();
    assert_eq!(antes.len(), depois.len(), "nada extraído");
}

#[test]
fn outros_tipos_nao_tem_previa() {
    let pasta = pasta();
    let caminho = pasta.path().join("programa.exe");
    std::fs::write(&caminho, b"MZ").expect("gravar");
    match abrir(&caminho, vec![]) {
        Conteudo::SemPrevia { motivo } => assert!(motivo.contains(".exe"), "{motivo}"),
        outro => panic!("{outro:?}"),
    }
}

fn tem(programa: &str) -> bool {
    sbf_processo::localizar(programa).is_some()
}

#[test]
fn heic_e_avif_pelo_ffmpeg_em_pipe() {
    if !tem("ffmpeg") {
        eprintln!("UNVERIFIED: sem ffmpeg");
        return;
    }
    let pasta = pasta();
    let png = pasta.path().join("base.png");
    gravar_imagem(&png, 64, 48);
    // Um AVIF (HEIF com AV1) fictício: o mesmo contêiner do HEIC. O ffmpeg não grava HEIC.
    let avif = pasta.path().join("foto.heic");
    let status = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&png)
        .args([
            "-frames:v",
            "1",
            "-c:v",
            "libaom-av1",
            "-still-picture",
            "1",
            "-f",
            "avif",
        ])
        .arg(&avif)
        .status()
        .expect("ffmpeg");
    if !status.success() {
        eprintln!("UNVERIFIED: o ffmpeg não gravou AVIF");
        return;
    }
    let inicio = Instant::now();
    let conteudo = abrir(&avif, vec![]);
    eprintln!("MEDIDA heic/avif: {} ms", inicio.elapsed().as_millis());
    let Conteudo::Imagem(i) = conteudo else {
        panic!("{conteudo:?}")
    };
    assert_eq!((i.largura, i.altura), (64, 48));
    let p = pixel(&i, 32, 5);
    assert!(p[0] > 150 && p[2] < 90, "{p:?}");
}

/// LibreOffice: um csv e um rtf convertidos em PDF numa pasta em memória, apagada na hora.
#[test]
fn office_vira_paginas_numa_pasta_em_memoria_apagada_na_hora() {
    if !tem("soffice") {
        eprintln!("UNVERIFIED: sem soffice");
        return;
    }
    let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") else {
        eprintln!("UNVERIFIED: sem XDG_RUNTIME_DIR");
        return;
    };
    let pasta = pasta();
    let rtf = pasta.path().join("carta.rtf");
    std::fs::write(
        &rtf,
        r"{\rtf1\ansi{\fonttbl\f0\fswiss Helvetica;}\f0\pard Carta ficticia de teste.\par}",
    )
    .expect("gravar");
    let inicio = Instant::now();
    let conteudo = sbf_previa::abrir(Pedido {
        arquivo: &rtf,
        extensao: "rtf",
        candidatas: vec![],
    })
    .expect("converter");
    eprintln!(
        "MEDIDA office: rtf convertido em {} ms",
        inicio.elapsed().as_millis()
    );
    let p = paginas(&conteudo);
    assert_eq!(p.total(), 1);
    let pagina = p.desenhar(1, 300).expect("desenhar");
    assert_eq!(pagina.largura, 300);

    // Nenhuma pasta de conversão sobrou, e nada apareceu ao lado do original.
    let raiz = std::path::Path::new(&xdg).join("sbf-previa");
    let eu = format!("conversao-{}-", std::process::id());
    let sobras: Vec<_> = std::fs::read_dir(&raiz)
        .expect("raiz")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&eu))
        .collect();
    assert!(sobras.is_empty(), "sobrou {sobras:?}");
    use std::os::unix::fs::PermissionsExt;
    let modo = std::fs::metadata(&raiz).expect("raiz").permissions().mode();
    assert_eq!(modo & 0o777, 0o700);
    let nomes: Vec<_> = std::fs::read_dir(pasta.path())
        .expect("ler")
        .flatten()
        .collect();
    assert_eq!(nomes.len(), 1, "{nomes:?}");

    let csv = pasta.path().join("planilha.csv");
    std::fs::write(&csv, "a,b\n1,2\n").expect("gravar");
    let inicio = Instant::now();
    let conteudo = sbf_previa::abrir(Pedido {
        arquivo: &csv,
        extensao: "ods",
        candidatas: vec![],
    });
    eprintln!(
        "MEDIDA office: csv (como ods) em {} ms: {:?}",
        inicio.elapsed().as_millis(),
        conteudo.as_ref().map(|c| matches!(c, Conteudo::Paginas(_)))
    );
}
