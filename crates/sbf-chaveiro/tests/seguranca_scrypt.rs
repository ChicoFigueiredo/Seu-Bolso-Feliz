//! Achado A3 da revisão de segurança da fatia 1: um `cofre.enc` forjado não pode fazer o
//! scrypt alocar gigabytes nem ocupar a CPU por segundos antes de dar o erro de sempre.
//! Também um `cofre.enc` gigante é recusado sem ser lido inteiro. Tudo fictício.

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SENHA_FICTICIA: &str = "senha-ficticia-1";

fn cofre_enc_forjado(caminho: &Path, n: u64, r: u64, p: u64) {
    let json = serde_json::json!({
        "versao": 1,
        "kdf": {"alg": "scrypt", "N": n, "r": r, "p": p, "sal": STANDARD.encode([7u8; 16])},
        "iv": STANDARD.encode([1u8; 12]),
        "tag": STANDARD.encode([2u8; 16]),
        "dados": STANDARD.encode([3u8; 64]),
    });
    std::fs::write(caminho, serde_json::to_vec(&json).expect("json")).expect("gravar");
}

fn cenario() -> (tempfile::TempDir, PathBuf, Chaveiro) {
    let base = tempfile::tempdir().expect("pasta");
    let enc = base.path().join("cofre/segredos/cofre.enc");
    std::fs::create_dir_all(enc.parent().expect("pai")).expect("pasta do cofre");
    let chaveiro = Chaveiro::abrir_com_chaveiro(
        &base.path().join("dados/chaveiro"),
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .expect("chaveiro");
    (base, enc, chaveiro)
}

fn importar_e_medir(chaveiro: &Chaveiro, enc: &Path) -> (Option<ErroDoChaveiro>, Duration) {
    let inicio = Instant::now();
    let erro = chaveiro.importar(enc, &Segredo::novo(SENHA_FICTICIA)).err();
    (erro, inicio.elapsed())
}

/// A sonda S1 da revisão usou r=2^20 e p=16 (2 GiB, 20 s). Aqui r=2^18 e p=16, que antes
/// alocava 512 MiB e levava segundos; agora é recusado antes de derivar.
#[test]
fn r_e_p_gigantes_sao_recusados_antes_de_derivar() {
    let (_base, enc, chaveiro) = cenario();
    cofre_enc_forjado(&enc, 2, 1 << 18, 16);
    let (erro, tempo) = importar_e_medir(&chaveiro, &enc);
    assert_eq!(erro, Some(ErroDoChaveiro::SenhaOuArquivoInvalido));
    assert!(tempo < Duration::from_millis(200), "levou {tempo:?}");
}

/// Fora das faixas que o `segredos.ts` gerava (com folga): o mesmo erro, rápido.
#[test]
fn parametros_fora_da_faixa_sao_arquivo_invalido() {
    let (_base, enc, chaveiro) = cenario();
    for (n, r, p) in [
        (1u64 << 19, 8, 1), // N acima de 2^18
        (1 << 4, 8, 1),     // N abaixo de 2^10
        (1 << 10, 64, 1),   // r acima de 16
        (1 << 10, 8, 8),    // p acima de 4
        (1 << 18, 16, 1),   // memória: 512 MiB
        (1 << 17, 16, 4),   // tempo: 8x o padrão do segredos.ts
    ] {
        cofre_enc_forjado(&enc, n, r, p);
        let (erro, tempo) = importar_e_medir(&chaveiro, &enc);
        assert_eq!(
            erro,
            Some(ErroDoChaveiro::SenhaOuArquivoInvalido),
            "N={n} r={r} p={p}"
        );
        assert!(
            tempo < Duration::from_millis(200),
            "N={n} r={r} p={p}: {tempo:?}"
        );
    }
}

/// Um `cofre.enc` de vários megabytes não é lido para a memória inteiro.
#[test]
fn arquivo_grande_demais_e_arquivo_invalido() {
    let (_base, enc, chaveiro) = cenario();
    let mut json = Vec::from(&b"{\"versao\":1,\"lixo\":\""[..]);
    json.resize(4 * 1024 * 1024, b'a');
    json.extend_from_slice(b"\"}");
    std::fs::write(&enc, json).expect("gravar");
    let (erro, _) = importar_e_medir(&chaveiro, &enc);
    assert_eq!(erro, Some(ErroDoChaveiro::SenhaOuArquivoInvalido));
}
