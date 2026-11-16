//! Captura de tela da própria janela, para conferir o visual sem ferramenta externa:
//! `SBF_CAPTURA=/caminho/tela.png sbf --demonstracao` desenha alguns quadros, salva a
//! janela em PNG e fecha. `SBF_CAPTURA_ABRIR=fornecedor:Energia Exemplo` abre um item
//! (ou `area:ATriar`) antes da captura.

use std::io::Write;
use std::path::Path;

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

/// Grava RGBA em PNG sem compressão (blocos "stored" do zlib).
pub fn salvar_png(
    caminho: &Path,
    largura: usize,
    altura: usize,
    rgba: &[u8],
) -> std::io::Result<()> {
    let mut cru = Vec::with_capacity((largura * 4 + 1) * altura);
    for y in 0..altura {
        cru.push(0);
        cru.extend_from_slice(&rgba[y * largura * 4..(y + 1) * largura * 4]);
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
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(largura as u32).to_be_bytes());
    ihdr.extend_from_slice(&(altura as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    bloco(&mut png, b"IHDR", &ihdr);
    bloco(&mut png, b"IDAT", &z);
    bloco(&mut png, b"IEND", &[]);
    std::fs::File::create(caminho)?.write_all(&png)
}
