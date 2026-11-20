//! O reconhecimento registra cada arquivo na Caixa de entrada (`caixa_entrada`), com o
//! canal, o tipo, o resumo e o estado do glossário. Cofre temporário, dados fictícios.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_inventario::caixa_de_entrada::{self, Canal, Chegada, Estado, Filtro, Situacao};
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_reconhecimento::{Reconhecedor, ReconhecedorDoCofre, Reconhecido};
use zeroize::Zeroizing;

const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");
const BOLETO: &str =
    "BOLETO DE COBRANCA ENERGIA EXEMPLO S.A. Vencimento 24/11/2026 Valor R$ 187,50";

struct Segredos;

impl FonteDeSegredos for Segredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new("52998224725".to_owned()))
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::SemSenhaPropria)
    }
}

struct Cenario {
    _raiz: tempfile::TempDir,
    cofre: PathBuf,
    soltos: PathBuf,
}

fn cenario() -> Cenario {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    let soltos = raiz.path().join("soltos");
    std::fs::create_dir_all(&cofre).expect("cofre");
    std::fs::create_dir_all(&soltos).expect("soltos");
    let mut inventario = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inventario
        .migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    Cenario {
        _raiz: raiz,
        cofre,
        soltos,
    }
}

impl Cenario {
    fn conexao(&self) -> Connection {
        Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir")
    }

    fn solto(&self, nome: &str, conteudo: &str) -> PathBuf {
        let c = self.soltos.join(nome);
        std::fs::write(&c, conteudo).expect("escrever");
        c
    }

    fn reconhecedor(&self) -> ReconhecedorDoCofre {
        ReconhecedorDoCofre::novo(&self.cofre, Arc::new(Segredos), None)
    }

    fn rodar(&self, r: &ReconhecedorDoCofre, caminho: &Path) -> sbf_reconhecimento::Resultado {
        r.reconhecer(caminho, &mut |_| {}, &AtomicBool::new(false))
            .expect("reconhecer")
    }

    fn fila(&self, filtro: &Filtro) -> Vec<caixa_de_entrada::Entrada> {
        caixa_de_entrada::listar(&self.conexao(), filtro).expect("listar")
    }

    /// Duas origens do conteúdo fora a fonte `arrastado` (o mesmo arquivo em duas pastas).
    fn achado_em_duas_pastas(&self, sha: &str) {
        let c = self.conexao();
        c.execute(
            "INSERT INTO fonte (id, tipo, descricao, raiz, escopo)
             VALUES ('disco:t', 'disco', 'teste', '/x', 'dentro')",
            [],
        )
        .expect("fonte");
        let varredura: i64 = c
            .query_row(
                "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, parametros)
                 VALUES ('disco:t', 'E1', '/x', 'teste', '{}') RETURNING id",
                [],
                |l| l.get(0),
            )
            .expect("varredura");
        for (i, pasta) in ["a", "b"].iter().enumerate() {
            c.execute(
                "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type,
                   origin_key, local_filepath, filename)
                 VALUES (?1, 'disco:t', ?2, 'local_file', ?3, ?4, 'x.txt')",
                rusqlite::params![sha, varredura, format!("k{i}"), format!("/x/{pasta}/x.txt")],
            )
            .expect("origem");
        }
    }
}

#[test]
fn sem_canal_da_caixa_nada_e_registrado() {
    let c = cenario();
    let a = c.solto("a.txt", CONTRACHEQUE);
    c.rodar(&c.reconhecedor(), &a);
    let n: i64 = c
        .conexao()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'caixa_entrada'",
            [],
            |l| l.get(0),
        )
        .expect("contar");
    assert_eq!(n, 1, "a tabela existe pela migração");
    assert!(c.fila(&Filtro::caixa()).is_empty());
    assert!(c.fila(&Filtro::sem_padrao()).is_empty());
}

#[test]
fn contracheque_entra_como_reconhecido_com_o_canal() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c.rodar(
        &c.reconhecedor().com_caixa_de_entrada(Canal::Cli, false),
        &a,
    );
    assert!(matches!(r.reconhecido, Reconhecido::Contracheque { .. }));
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    let e = &fila[0];
    assert_eq!(e.content_hash, r.sha256);
    assert_eq!(e.nome, "contracheque.txt");
    assert_eq!(e.canal, Canal::Cli);
    assert_eq!(e.tipo.as_deref(), Some("Contracheque"));
    assert!(e.estado.e_reconhecido(), "{:?}", e.estado);
    assert_eq!(e.situacao, Situacao::Aberta);
    assert!(c.fila(&Filtro::sem_padrao()).is_empty());
}

#[test]
fn o_que_nenhuma_regra_casa_vai_para_sem_padrao() {
    let c = cenario();
    let a = c.solto("boleto.txt", BOLETO);
    let r = c.rodar(
        &c.reconhecedor()
            .com_caixa_de_entrada(Canal::Telegram, false),
        &a,
    );
    assert!(matches!(r.reconhecido, Reconhecido::Nada { .. }));
    assert!(c.fila(&Filtro::caixa()).is_empty());
    let sem = c.fila(&Filtro::sem_padrao());
    assert_eq!(sem.len(), 1);
    assert_eq!(sem[0].estado, Estado::NaoReconhecido);
    assert_eq!(sem[0].canal, Canal::Telegram);
    let n = caixa_de_entrada::contagens(&c.conexao()).expect("contagens");
    assert_eq!((n.reconhecidos, n.sem_padrao), (0, 1));
}

#[test]
fn reconhecer_de_novo_e_idempotente_e_guarda_o_primeiro_canal() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    c.rodar(
        &c.reconhecedor()
            .com_caixa_de_entrada(Canal::Telegram, false),
        &a,
    );
    c.rodar(
        &c.reconhecedor().com_caixa_de_entrada(Canal::Pasta, false),
        &a,
    );
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].canal, Canal::Telegram);
    assert_eq!(fila[0].vezes, 2);
}

#[test]
fn o_canal_gmail_registra_sem_pedir_o_canal() {
    let c = cenario();
    let a = c.solto("fatura.txt", CONTRACHEQUE);
    c.reconhecedor()
        .reconhecer_de(
            Some((sbf_regras::Canal::Gmail, "msg-1".to_owned())),
            &a,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .expect("reconhecer");
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].canal, Canal::Gmail);
}

#[test]
fn resolvido_so_reabre_por_acao_do_usuario() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c.rodar(
        &c.reconhecedor()
            .com_caixa_de_entrada(Canal::Telegram, false),
        &a,
    );
    caixa_de_entrada::decidir(&c.conexao(), &r.sha256, Situacao::Resolvida).expect("resolver");
    // Chega de novo, sozinho (Gmail): continua resolvido.
    c.rodar(
        &c.reconhecedor().com_caixa_de_entrada(Canal::Gmail, false),
        &a,
    );
    assert!(c.fila(&Filtro::caixa()).is_empty());
    // `sbf reconhecer` (ação do usuário) reabre.
    c.rodar(&c.reconhecedor().com_caixa_de_entrada(Canal::Cli, true), &a);
    assert_eq!(c.fila(&Filtro::caixa()).len(), 1);
}

#[test]
fn o_mesmo_conteudo_em_duas_pastas_e_possivel_duplicidade() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c.rodar(&c.reconhecedor(), &a);
    c.achado_em_duas_pastas(&r.sha256);
    c.rodar(
        &c.reconhecedor().com_caixa_de_entrada(Canal::Pasta, false),
        &a,
    );
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].estado, Estado::PossivelDuplicidade);
}

#[test]
fn reconhecer_conteudo_le_a_copia_do_acervo_e_nao_mexe_na_fila_sem_origem() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c.rodar(
        &c.reconhecedor()
            .com_caixa_de_entrada(Canal::Telegram, false),
        &a,
    );
    // O original some: o acervo do cofre ainda tem o arquivo.
    std::fs::remove_file(&a).expect("apagar");
    let de_novo = c
        .reconhecedor()
        .reconhecer_conteudo(&r.sha256, &mut |_| {}, &AtomicBool::new(false))
        .expect("do acervo");
    assert_eq!(de_novo.sha256, r.sha256);
    assert!(matches!(
        de_novo.reconhecido,
        Reconhecido::Contracheque { .. }
    ));
    assert_eq!(
        c.fila(&Filtro::caixa())[0].vezes,
        1,
        "sem origem da fila, não registra"
    );
    let ausente =
        c.reconhecedor()
            .reconhecer_conteudo(&"0".repeat(64), &mut |_| {}, &AtomicBool::new(false));
    assert!(ausente.is_err());
}

#[test]
fn reconhecer_de_novo_so_atualiza_nunca_cria_nem_reabre() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c.rodar(
        &c.reconhecedor()
            .com_caixa_de_entrada(Canal::Telegram, false),
        &a,
    );
    caixa_de_entrada::decidir(&c.conexao(), &r.sha256, Situacao::Dispensada).expect("dispensar");
    let de_novo = c.reconhecedor().atualizando_a_caixa_de_entrada();
    c.rodar(&de_novo, &a);
    let e = caixa_de_entrada::obter(&c.conexao(), &r.sha256)
        .expect("obter")
        .expect("existe");
    assert_eq!(
        e.situacao,
        Situacao::Dispensada,
        "não reabre o que foi dispensado"
    );
    assert_eq!(e.canal, Canal::Telegram);
    assert_eq!(e.vezes, 2);
    // Um conteúdo que nunca esteve na Caixa de entrada não ganha linha.
    let outro = c.solto("outro.txt", "OUTRO DOCUMENTO FICTICIO");
    let o = c.rodar(&de_novo, &outro);
    assert!(
        caixa_de_entrada::obter(&c.conexao(), &o.sha256)
            .expect("obter")
            .is_none()
    );
}

#[test]
fn a_leitura_que_falha_vai_para_precisa_revisar_e_nao_para_sem_padrao() {
    let c = cenario();
    // Um "PDF" que não abre: a leitura falha, não é falta de padrão.
    let a = c.solto("quebrado.pdf", "%PDF-1.4 isto nao e um pdf de verdade");
    let r = c.rodar(
        &c.reconhecedor().com_caixa_de_entrada(Canal::Pasta, false),
        &a,
    );
    assert!(
        matches!(r.texto, sbf_reconhecimento::EstadoDoTexto::Falhou(_)),
        "{:?}",
        r.texto
    );
    assert!(c.fila(&Filtro::sem_padrao()).is_empty());
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].estado, Estado::PrecisaRevisar);
    assert_eq!(
        fila[0].resumo.as_deref(),
        Some(sbf_reconhecimento::caixa_de_entrada::RESUMO_LEITURA_FALHOU)
    );
}

#[test]
fn a_falha_antes_do_conteudo_ser_lido_tambem_pede_revisao() {
    let c = cenario();
    let r = c.reconhecedor();
    let chegada = Chegada {
        canal: Canal::Celular,
        reabrir: false,
    };
    r.registrar_falha_da_leitura(chegada, &"ab".repeat(32), "sumiu.pdf");
    let fila = c.fila(&Filtro::caixa());
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].estado, Estado::PrecisaRevisar);
    assert_eq!(fila[0].canal, Canal::Celular);
    assert!(c.fila(&Filtro::sem_padrao()).is_empty());
}

#[test]
fn so_com_texto_guardado_nao_extrai_e_o_cartao_diz_que_falta_texto() {
    let c = cenario();
    let a = c.solto("contracheque.txt", CONTRACHEQUE);
    let r = c
        .reconhecedor()
        .so_com_texto_guardado()
        .reconhecer(&a, &mut |_| {}, &AtomicBool::new(false))
        .expect("reconhecer");
    match &r.texto {
        sbf_reconhecimento::EstadoDoTexto::SemTexto(m) => {
            assert!(m.contains("sem texto guardado"), "{m}");
        }
        outro => panic!("esperava SemTexto, veio {outro:?}"),
    }
    let n: i64 = c
        .conexao()
        .query_row("SELECT COUNT(*) FROM texto", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(n, 0, "nada foi extraído nem gravado");
    // Com o texto já guardado, o mesmo reconhecedor lê e reconhece.
    c.rodar(&c.reconhecedor(), &a);
    let r = c
        .reconhecedor()
        .so_com_texto_guardado()
        .reconhecer(&a, &mut |_| {}, &AtomicBool::new(false))
        .expect("reconhecer");
    assert!(matches!(r.reconhecido, Reconhecido::Contracheque { .. }));
}

#[test]
fn a_fonte_arrastada_e_a_mesma_da_coleta() {
    assert_eq!(
        caixa_de_entrada::FONTE_ARRASTADO,
        sbf_coleta::pastas::FONTE_ARRASTADO
    );
}

#[test]
fn o_canal_das_regras_vira_o_da_caixa_sem_perder_nenhum() {
    use sbf_reconhecimento::caixa_de_entrada::canal_da_caixa;
    use sbf_regras::Canal as R;
    assert_eq!(canal_da_caixa(R::Gmail), Some(Canal::Gmail));
    assert_eq!(canal_da_caixa(R::Telegram), Some(Canal::Telegram));
    assert_eq!(canal_da_caixa(R::Pasta), Some(Canal::Pasta));
    assert_eq!(canal_da_caixa(R::Arrastado), Some(Canal::Arrastado));
    for fora in [R::Cofre, R::Pluggy, R::Mcp, R::Manual] {
        assert_eq!(canal_da_caixa(fora), None);
    }
}
