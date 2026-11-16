//! O reconhecedor real sobre um cofre temporário: inventário migrado, planejamento com um
//! Salário e arquivos soltos fictícios. Nenhuma IA de verdade: só o provedor falso.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use rusqlite::Connection;
use sbf_ia::{Consumo, ErroDeIa, Falso, Provedor, Resposta, ResultadoDeIa};
use sbf_inventario::banco::Inventario;
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Competencia, Fonte, Frequencia, Natureza,
    NovoRecorrente, Planejamento, PlanejamentoSqlite, Prazo, SemSaldos, Versao,
};
use sbf_reconhecimento::{
    Andamento, ErroDoReconhecimento, EstadoDoTexto, Etapa, PROVAS_POSSIVEIS, Reconhecedor,
    ReconhecedorDoCofre, Reconhecido, Resultado, Situacao, Sugestao,
};
use zeroize::Zeroizing;

const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");
const CONTRACHEQUE_2026: &str =
    include_str!("../../sbf-planilha/tests/contracheque-2026-ficticio.txt");
const CPF: &str = "52998224725";

struct Segredos;

impl FonteDeSegredos for Segredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new(CPF.to_owned()))
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

    fn solto(&self, nome: &str, conteudo: &[u8]) -> PathBuf {
        let c = self.soltos.join(nome);
        std::fs::write(&c, conteudo).expect("escrever");
        c
    }

    fn contar(&self, sql: &str) -> i64 {
        self.conexao()
            .query_row(sql, [], |l| l.get(0))
            .expect("contar")
    }

    fn reconhecedor(&self, provedor: Option<Arc<dyn Provedor>>) -> ReconhecedorDoCofre {
        ReconhecedorDoCofre::novo(&self.cofre, Arc::new(Segredos), provedor)
    }

    fn rodar(
        &self,
        r: &ReconhecedorDoCofre,
        caminho: &Path,
    ) -> (Result<Resultado, ErroDoReconhecimento>, Vec<Andamento>) {
        let mut vistos = Vec::new();
        let saida = r.reconhecer(caminho, &mut |a| vistos.push(a), &AtomicBool::new(false));
        (saida, vistos)
    }

    /// Um planejamento com o recorrente "Salário" de R$ 5.699,50 desde 09/2026.
    fn planejamento_com_salario(&self, valor: i64) -> String {
        let banco = Banco::abrir(self.cofre.join("planejamento.sqlite")).expect("banco");
        let mut plan = PlanejamentoSqlite::novo(banco, Box::new(SemSaldos));
        let recibo = plan
            .aplicar(
                Alteracao::CriarRecorrente {
                    recorrente: NovoRecorrente {
                        nome: "Salário".into(),
                        grupo: "Salário".into(),
                        subgrupo: None,
                        posicao: 0,
                        natureza: Natureza::Entrada,
                        prazo: Prazo::SemFim,
                        primeira_versao: Versao {
                            id: String::new(),
                            desde: Competencia { ano: 2026, mes: 9 },
                            valor: Some(Centavos(valor)),
                            certeza: Certeza::Estimado,
                            fonte: Fonte::Usuario,
                            frequencia: Frequencia::Mensal { dia: 5 },
                            conta: Some("CX-0630".into()),
                            centro: None,
                            fornecedor: None,
                            categoria: None,
                            nota: None,
                        },
                    },
                    origem: None,
                },
                Autor::Usuario,
            )
            .expect("criar");
        drop(recibo);
        plan.recorrentes(false).expect("lista")[0].id.clone()
    }

    fn plan(&self) -> PlanejamentoSqlite {
        let banco = Banco::abrir(self.cofre.join("planejamento.sqlite")).expect("banco");
        PlanejamentoSqlite::novo(banco, Box::new(SemSaldos))
    }
}

fn ok(r: Result<Resultado, ErroDoReconhecimento>) -> Resultado {
    r.expect("reconhecer")
}

fn situacao_de(vistos: &[Andamento], etapa: Etapa) -> Situacao {
    vistos
        .iter()
        .rev()
        .find(|a| a.etapa == etapa)
        .map(|a| a.situacao.clone())
        .expect("etapa anunciada")
}

fn resposta(json: serde_json::Value) -> ResultadoDeIa<Resposta> {
    Ok(Resposta {
        json,
        consumo: Consumo {
            tokens_entrada: 100,
            tokens_saida: 20,
            custo_dezmilesimos_usd: 0,
            nominal: true,
        },
        modelo: "falso-modelo".into(),
    })
}

fn ia_de_boleto() -> Arc<Falso> {
    Arc::new(Falso::com_funcao(|_| {
        resposta(serde_json::json!({"itens": [{"n": 1, "sugestoes": [
            {"campo": "e_financeiro", "valor": "sim", "confianca": 0.98, "justificativa": "boleto"},
            {"campo": "tipo_documento", "valor": "boleto", "confianca": 0.97, "justificativa": "linha digitável"},
            {"campo": "fornecedor", "valor": "Energia Exemplo", "confianca": 0.96, "justificativa": "emissor"},
            {"campo": "data_chave", "valor": "2026-11-24", "confianca": 0.9, "justificativa": "vencimento"}
        ]}]}))
    }))
}

const BOLETO: &str = "BOLETO DE COBRANCA ENERGIA EXEMPLO S.A. Vencimento 24/11/2026 Valor R$ 187,50 \
                      Linha digitavel 23790.12345 60000.123456 78901.234567 8 99990000018750";

/// Um PDF fictício de uma página com as linhas dadas (Helvetica, WinAnsi).
fn pdf_com_linhas(linhas: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let mut ops = Vec::new();
    for (i, l) in linhas.iter().enumerate() {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new("Tf", vec!["F1".into(), 11.into()]));
        ops.push(Operation::new(
            "Td",
            vec![60.into(), (780 - 20 * i as i64).into()],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::string_literal(l.as_bytes().to_vec())],
        ));
        ops.push(Operation::new("ET", vec![]));
    }
    let conteudo = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("conteúdo"),
    ));
    let pagina = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => conteudo,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![pagina.into()], "Count" => 1,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalogo = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalogo);
    let mut saida = Vec::new();
    doc.save_to(&mut saida).expect("salvar");
    saida
}

#[test]
fn arquivo_novo_registra_copia_ao_acervo_e_deixa_o_original_intacto() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &original);
    let r = ok(r);

    assert!(!r.ja_existia);
    assert_eq!(r.nome, "conta.txt");
    assert_eq!(r.sha256.len(), 64);
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 1);
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM origem WHERE fonte_id = 'arrastado'"),
        1
    );
    let acervo: String = c
        .conexao()
        .query_row("SELECT caminho_acervo FROM achado", [], |l| l.get(0))
        .expect("acervo");
    assert_eq!(
        std::fs::read(c.cofre.join(&acervo)).expect("cópia"),
        BOLETO.as_bytes()
    );
    assert_eq!(
        std::fs::read(&original).expect("original"),
        BOLETO.as_bytes()
    );
    assert_eq!(situacao_de(&vistos, Etapa::Registrar), Situacao::Feita);
    assert_eq!(r.texto, EstadoDoTexto::Extraido);
}

#[test]
fn as_etapas_sao_anunciadas_em_ordem_cada_uma_com_rodando_e_o_desfecho() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &original);
    ok(r);

    assert_eq!(vistos.len(), 12);
    for (i, etapa) in Etapa::TODAS.iter().enumerate() {
        assert_eq!(vistos[2 * i].etapa, *etapa);
        assert_eq!(vistos[2 * i].situacao, Situacao::Rodando);
        assert_eq!(vistos[2 * i + 1].etapa, *etapa);
        assert_ne!(vistos[2 * i + 1].situacao, Situacao::Rodando);
    }
    // Pulada também é anunciada.
    assert!(matches!(
        situacao_de(&vistos, Etapa::PerguntarAIa),
        Situacao::Pulada(_)
    ));
}

#[test]
fn arquivo_ja_no_inventario_e_ja_existia_e_nao_duplica() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let r = c.reconhecedor(None);

    let primeira = ok(c.rodar(&r, &original).0);
    let (segunda, vistos) = c.rodar(&r, &original);
    let segunda = ok(segunda);

    assert!(!primeira.ja_existia && segunda.ja_existia);
    assert_eq!(primeira.sha256, segunda.sha256);
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 1);
    assert_eq!(c.contar("SELECT COUNT(*) FROM origem"), 1);
    assert_eq!(c.contar("SELECT COUNT(*) FROM texto"), 1);
    assert!(matches!(
        situacao_de(&vistos, Etapa::Registrar),
        Situacao::Pulada(_)
    ));
}

#[test]
fn o_mesmo_conteudo_em_outro_caminho_ja_existia_e_ganha_so_uma_origem() {
    let c = cenario();
    let a = c.solto("a.txt", BOLETO.as_bytes());
    let b = c.solto("b.txt", BOLETO.as_bytes());
    let r = c.reconhecedor(None);

    ok(c.rodar(&r, &a).0);
    let segunda = ok(c.rodar(&r, &b).0);

    assert!(segunda.ja_existia);
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 1);
    assert_eq!(c.contar("SELECT COUNT(*) FROM origem"), 2);
}

#[test]
fn achado_conhecido_sem_copia_ganha_a_copia_agora() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let hash = sbf_coleta::pastas::sha256_do_arquivo(&original).expect("hash");
    c.conexao()
        .execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 10, 'B')",
            [&hash],
        )
        .expect("achado");

    let r = ok(c.rodar(&c.reconhecedor(None), &original).0);

    assert!(r.ja_existia);
    let acervo: Option<String> = c
        .conexao()
        .query_row("SELECT caminho_acervo FROM achado", [], |l| l.get(0))
        .expect("acervo");
    assert!(c.cofre.join(acervo.expect("copiou")).is_file());
}

#[test]
fn texto_que_o_inventario_ja_tinha_vira_ja_havia_e_nao_e_regravado() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let hash = sbf_coleta::pastas::sha256_do_arquivo(&original).expect("hash");
    let conn = c.conexao();
    conn.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 10, 'A')",
        [&hash],
    )
    .expect("achado");
    conn.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'ocr-por', 'texto antigo')",
        [&hash],
    )
    .expect("texto");

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &original);
    let r = ok(r);

    assert_eq!(r.texto, EstadoDoTexto::JaHavia);
    assert!(matches!(
        situacao_de(&vistos, Etapa::ExtrairTexto),
        Situacao::Pulada(_)
    ));
    let (metodo, conteudo): (String, String) = conn
        .query_row("SELECT metodo, conteudo FROM texto", [], |l| {
            Ok((l.get(0)?, l.get(1)?))
        })
        .expect("texto");
    assert_eq!(
        (metodo.as_str(), conteudo.as_str()),
        ("ocr-por", "texto antigo")
    );
}

#[test]
fn pdf_sem_senha_grava_o_texto_e_a_impressao_canonica() {
    let c = cenario();
    let pdf = c.solto(
        "fatura.pdf",
        &pdf_com_linhas(&[
            "BANCO ALFA FICTICIO - FATURA DO CARTAO",
            "Vencimento 10/12/2026 - Valor R$ 1.234,56",
        ]),
    );

    let r = ok(c.rodar(&c.reconhecedor(None), &pdf).0);

    assert_eq!(r.texto, EstadoDoTexto::Extraido);
    let (metodo, conteudo, impressao): (String, String, Option<String>) = c
        .conexao()
        .query_row(
            "SELECT t.metodo, t.conteudo, a.canonical_fingerprint
               FROM texto t JOIN achado a USING (content_hash)",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .expect("texto");
    assert_eq!(metodo, "pdftotext");
    assert!(conteudo.contains("FATURA DO CARTAO"), "{conteudo}");
    assert_eq!(
        impressao.as_deref(),
        Some(sbf_pdf::impressao_canonica(&conteudo).as_str())
    );
}

#[test]
fn pdf_sem_texto_grava_o_marcador_e_nao_vai_para_a_ia() {
    let c = cenario();
    let pdf = c.solto("imagem.pdf", &pdf_com_linhas(&[]));
    let falso = ia_de_boleto();

    let (r, vistos) = c.rodar(&c.reconhecedor(Some(falso.clone())), &pdf);
    let r = ok(r);

    assert!(
        matches!(r.texto, EstadoDoTexto::SemTexto(_)),
        "{:?}",
        r.texto
    );
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM texto WHERE metodo = 'pdf-sem-texto'"),
        1
    );
    assert!(matches!(
        situacao_de(&vistos, Etapa::PerguntarAIa),
        Situacao::Pulada(_)
    ));
    assert_eq!(falso.chamadas(), 0);
}

#[test]
fn formato_sem_leitor_diz_o_motivo() {
    let c = cenario();
    let f = c.solto("foto.jpg", b"nao e uma foto de verdade");

    let r = ok(c.rodar(&c.reconhecedor(None), &f).0);

    assert_eq!(
        r.texto,
        EstadoDoTexto::SemTexto("ainda sem leitor de texto para este formato".to_owned())
    );
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 1);
}

#[test]
fn contracheque_gera_vincular_e_usar_rubricas_com_a_divergencia() {
    let c = cenario();
    let id = c.planejamento_com_salario(550_000);
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE.as_bytes());
    let falso = ia_de_boleto();

    let (r, vistos) = c.rodar(&c.reconhecedor(Some(falso.clone())), &arquivo);
    let r = ok(r);

    assert!(
        matches!(r.reconhecido, Reconhecido::Contracheque { .. }),
        "{:?}",
        r.reconhecido
    );
    assert_eq!(
        falso.chamadas(),
        0,
        "a IA só entra quando o local não reconhece"
    );
    assert!(matches!(
        situacao_de(&vistos, Etapa::PerguntarAIa),
        Situacao::Pulada(_)
    ));
    assert_eq!(r.sugestoes.len(), 2, "{:?}", r.sugestoes);
    match &r.sugestoes[0] {
        Sugestao::Vincular {
            alvo, rotulo, tipo, ..
        } => {
            assert_eq!(rotulo, "Vincular ao Salário · 10/2026");
            assert_eq!(*tipo, sbf_planejamento::TipoDeDocumento::Contracheque);
            assert_eq!(
                *alvo,
                sbf_reconhecimento::ocorrencia_do_mes(&id, Competencia { ano: 2026, mes: 10 })
            );
        }
        outra => panic!("{outra:?}"),
    }
    match &r.sugestoes[1] {
        Sugestao::UsarRubricas { rotulo, uso } => {
            assert_eq!(rotulo, "Usar estas 9 rubricas no Salário desde 10/2026");
            let marcacao = uso.eventuais_iniciais();
            let composicoes = uso.composicoes(&marcacao);
            assert_eq!(composicoes.len(), 1);
            assert_eq!(composicoes[0].desde, Competencia { ano: 2026, mes: 10 });
            assert_eq!(uso.divergencia(&marcacao), Some(Centavos(19_950)));
        }
        outra => panic!("{outra:?}"),
    }
    assert!(r.avisos.is_empty(), "{:?}", r.avisos);
    // Nada mudou no planejamento.
    assert!(
        c.plan()
            .composicao(&id, Competencia { ano: 2026, mes: 10 })
            .expect("c")
            .is_none()
    );
}

#[test]
fn contracheque_ja_vinculado_e_com_a_mesma_composicao_nao_sugere_nada() {
    let c = cenario();
    let id = c.planejamento_com_salario(569_950);
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE.as_bytes());
    let r = c.reconhecedor(None);

    let primeira = ok(c.rodar(&r, &arquivo).0);
    // O usuário confirma as duas sugestões.
    let mut plan = c.plan();
    let versao = plan
        .recorrente(&id)
        .expect("r")
        .expect("existe")
        .versao_do_registro;
    for s in &primeira.sugestoes {
        for a in s.alteracoes(&primeira.sha256, &primeira.nome, versao, None) {
            plan.aplicar(a, Autor::Usuario).expect("aplicar");
        }
    }
    drop(plan);

    let segunda = ok(c.rodar(&r, &arquivo).0);

    assert_eq!(primeira.sugestoes.len(), 2);
    assert_eq!(
        primeira.sugestoes[1].recorrente_para_versao(),
        Some(id.as_str())
    );
    assert!(segunda.sugestoes.is_empty(), "{:?}", segunda.sugestoes);
    assert!(matches!(
        segunda.reconhecido,
        Reconhecido::Contracheque { .. }
    ));
}

#[test]
fn contracheque_sem_recorrente_do_salario_avisa_e_nao_sugere() {
    let c = cenario();
    drop(Banco::abrir(c.cofre.join("planejamento.sqlite")).expect("banco"));
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert!(r.sugestoes.is_empty());
    assert_eq!(r.avisos.len(), 1, "{:?}", r.avisos);
    assert!(r.avisos[0].contains("recorrente do salário"));
}

#[test]
fn contracheque_sem_planejamento_pula_a_etapa_de_sugerir() {
    let c = cenario();
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE.as_bytes());

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &arquivo);

    assert!(ok(r).sugestoes.is_empty());
    assert!(matches!(
        situacao_de(&vistos, Etapa::Sugerir),
        Situacao::Pulada(_)
    ));
}

#[test]
fn contracheque_nao_conferido_traz_aviso() {
    let c = cenario();
    c.planejamento_com_salario(569_950);
    let adulterado = CONTRACHEQUE.replace("R$ 5.699,50", "R$ 5.000,00");
    let arquivo = c.solto("contracheque.txt", adulterado.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert!(
        r.avisos
            .iter()
            .any(|a| a.contains("não bate com o líquido")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn nao_contracheque_com_provedor_falso_vira_pela_ia_e_nao_repete_a_chamada() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let falso = ia_de_boleto();
    let r = c.reconhecedor(Some(falso.clone()));

    let (primeira, vistos) = c.rodar(&r, &original);
    let primeira = ok(primeira);

    match &primeira.reconhecido {
        Reconhecido::PelaIa {
            id_da_sugestao,
            resumo,
        } => {
            assert_eq!(resumo, "Boleto · Energia Exemplo · data 24/11/2026");
            let campo: String = c
                .conexao()
                .query_row(
                    "SELECT campo FROM sugestao_ia WHERE id = ?1",
                    [id_da_sugestao],
                    |l| l.get(0),
                )
                .expect("sugestão");
            assert_eq!(campo, "tipo_documento");
        }
        outro => panic!("{outro:?}"),
    }
    assert_eq!(situacao_de(&vistos, Etapa::PerguntarAIa), Situacao::Feita);
    assert_eq!(falso.chamadas(), 1);
    // Mesmo destinatário e nível da área Triagem; nada virou decisão.
    assert_eq!(
        c.conexao()
            .query_row("SELECT destinatario||'|'||nivel FROM saida_ia", [], |l| {
                l.get::<_, String>(0)
            })
            .expect("saida"),
        "triagem|completo"
    );
    assert_eq!(c.contar("SELECT COUNT(*) FROM decisao"), 0);
    // A IA só viu o texto da tabela, não o arquivo; o pedido não leva o caminho.
    assert!(!falso.pedidos()[0].entrada.contains("soltos"));

    let segunda = ok(c.rodar(&r, &original).0);
    assert!(matches!(segunda.reconhecido, Reconhecido::PelaIa { .. }));
    assert_eq!(falso.chamadas(), 1, "a sugestão aberta é reaproveitada");
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM sugestao_ia WHERE campo = 'tipo_documento'"),
        1
    );
}

#[test]
fn sem_provedor_devolve_nada_com_motivo_e_pula_a_ia() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &original);

    match ok(r).reconhecido {
        Reconhecido::Nada { motivo } => assert!(motivo.contains("IA não está configurada")),
        outro => panic!("{outro:?}"),
    }
    assert_eq!(
        situacao_de(&vistos, Etapa::PerguntarAIa),
        Situacao::Pulada("a IA não está configurada".to_owned())
    );
}

#[test]
fn ia_indisponivel_falha_a_etapa_sem_derrubar_o_resto() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let falso: Arc<dyn Provedor> = Arc::new(Falso::com_funcao(|_| {
        Err(ErroDeIa::Indisponivel("sem claude".to_owned()))
    }));

    let (r, vistos) = c.rodar(&c.reconhecedor(Some(falso)), &original);
    let r = ok(r);

    assert!(
        matches!(r.reconhecido, Reconhecido::Nada { .. }),
        "{:?}",
        r.reconhecido
    );
    assert!(matches!(
        situacao_de(&vistos, Etapa::PerguntarAIa),
        Situacao::Falhou(_)
    ));
}

#[test]
fn cancelar_para_entre_as_etapas() {
    let c = cenario();
    let original = c.solto("conta.txt", BOLETO.as_bytes());
    let cancelar = AtomicBool::new(false);
    let mut vistos = Vec::new();

    let saida = c.reconhecedor(None).reconhecer(
        &original,
        &mut |a| {
            if a.etapa == Etapa::Conferir && a.situacao == Situacao::Feita {
                cancelar.store(true, Ordering::SeqCst);
            }
            vistos.push(a);
        },
        &cancelar,
    );

    assert_eq!(saida, Err(ErroDoReconhecimento::Cancelado));
    assert_eq!(vistos.len(), 2, "só a primeira etapa rodou: {vistos:?}");
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 0);
}

#[test]
fn arquivo_de_dentro_do_cofre_so_confere_e_nao_registra_de_novo() {
    let c = cenario();
    let dentro = c.cofre.join("anotacao.txt");
    std::fs::write(&dentro, BOLETO).expect("escrever");

    let (r, vistos) = c.rodar(&c.reconhecedor(None), &dentro);
    let r = ok(r);

    assert!(!r.ja_existia);
    assert_eq!(c.contar("SELECT COUNT(*) FROM achado"), 0);
    assert_eq!(c.contar("SELECT COUNT(*) FROM origem"), 0);
    assert!(matches!(
        situacao_de(&vistos, Etapa::Registrar),
        Situacao::Pulada(_)
    ));
    assert!(matches!(r.texto, EstadoDoTexto::SemTexto(_)));
}

#[test]
fn sem_inventario_ou_arquivo_inexistente_e_erro_claro() {
    let c = cenario();
    let (saida, _) = c.rodar(&c.reconhecedor(None), &c.soltos.join("nao-existe.txt"));
    assert!(matches!(saida, Err(ErroDoReconhecimento::Arquivo(_))));
    let (saida, _) = c.rodar(&c.reconhecedor(None), &c.soltos);
    assert!(matches!(saida, Err(ErroDoReconhecimento::Arquivo(_))));
    // Cofre sem inventario.sqlite.
    let vazio = tempfile::tempdir().expect("vazio");
    let r = ReconhecedorDoCofre::novo(vazio.path(), Arc::new(Segredos), None);
    let arquivo = c.solto("a.txt", b"x");
    let (saida, _) = c.rodar(&r, &arquivo);
    assert!(matches!(saida, Err(ErroDoReconhecimento::Banco(_))));
}

/// PDF cifrado com `qpdf --encrypt` (a senha fictícia vai no argv só neste teste).
fn pdf_protegido(c: &Cenario, nome: &str, senha: &str) -> PathBuf {
    let base = c.soltos.join(format!("{nome}-base.pdf"));
    std::fs::write(
        &base,
        pdf_com_linhas(&["FATURA ALFA FICTICIA vencimento 10/12/2026"]),
    )
    .expect("base");
    let saida = c.soltos.join(format!("{nome}.pdf"));
    let r = std::process::Command::new("qpdf")
        .arg("--encrypt")
        .arg(format!("--user-password={senha}"))
        .arg("--owner-password=dono-ficticio-77")
        .arg("--bits=256")
        .arg("--")
        .arg(&base)
        .arg(&saida)
        .output()
        .expect("qpdf instalado");
    assert!(matches!(r.status.code(), Some(0 | 3)), "{r:?}");
    std::fs::remove_file(base).expect("limpar");
    saida
}

#[test]
fn pdf_protegido_sem_regra_vira_protegido_e_marca_o_achado() {
    let c = cenario();
    let pdf = pdf_protegido(&c, "protegida", "24725");

    let (r, vistos) = c.rodar(&c.reconhecedor(Some(ia_de_boleto())), &pdf);
    let r = ok(r);

    assert!(
        matches!(r.texto, EstadoDoTexto::Protegido(_)),
        "{:?}",
        r.texto
    );
    assert!(matches!(
        situacao_de(&vistos, Etapa::ExtrairTexto),
        Situacao::Falhou(_)
    ));
    assert_eq!(
        c.contar("SELECT COUNT(*) FROM achado WHERE protegido_senha = 1"),
        1
    );
    assert_eq!(c.contar("SELECT COUNT(*) FROM texto"), 0);
    assert!(matches!(r.reconhecido, Reconhecido::Nada { .. }));
}

#[test]
fn pdf_protegido_com_regra_do_chaveiro_abre_e_grava_o_texto() {
    let c = cenario();
    let pdf = pdf_protegido(&c, "protegida", "24725");
    // O inventário já conhece o fornecedor e tem a regra (o caso de sempre).
    let hash = sbf_coleta::pastas::sha256_do_arquivo(&pdf).expect("hash");
    let conn = c.conexao();
    conn.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, document_type, supplier_name_raw)
         VALUES (?1, 10, 'A', 'fatura', 'Banco Alfa')",
        [&hash],
    )
    .expect("achado");
    conn.execute(
        "INSERT INTO regra_senha (fornecedor, document_type, estrategia, prioridade)
         VALUES ('Banco Alfa', 'fatura', 'cpf_ultimos:5', 1)",
        [],
    )
    .expect("regra");

    let r = ok(c.rodar(&c.reconhecedor(None), &pdf).0);

    assert_eq!(r.texto, EstadoDoTexto::Extraido);
    let conteudo: String = conn
        .query_row("SELECT conteudo FROM texto", [], |l| l.get(0))
        .expect("texto");
    assert!(conteudo.contains("FATURA ALFA FICTICIA"), "{conteudo}");
    // A senha não foi parar no banco.
    let tudo: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tentativa_senha WHERE estrategia LIKE '%24725%'",
            [],
            |l| l.get(0),
        )
        .expect("tentativas");
    assert_eq!(tudo, 0);
}

fn conciliar_sugerido(r: &Resultado) -> Option<&Sugestao> {
    r.sugestoes
        .iter()
        .find(|s| matches!(s, Sugestao::Conciliar { .. }))
}

/// Um crédito no extrato do inventário (a Pluggy).
fn credito(c: &Cenario, id: &str, data: &str, valor: i64, conta: &str, descricao: &str) {
    let conn = c.conexao();
    conn.execute(
        "INSERT OR IGNORE INTO fonte (id, tipo, descricao) VALUES ('conexao:pluggy', 'conexao', 'x')",
        [],
    )
    .expect("fonte");
    conn.execute(
        "INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, valor_centavos,
                               descricao_original, conta_provavel, extraido_por)
         VALUES (?1, 'conexao:pluggy', 'credito', ?2, 'pagamento', ?3, ?4, ?5, 'pluggy@1')",
        rusqlite::params![id, data, valor, descricao, conta],
    )
    .expect("registro");
}

const LIQUIDO_DE_2026: i64 = 2_460_522;

#[test]
fn contracheque_de_2026_sugere_vincular_e_duas_composicoes_e_nada_de_recebimento_sem_credito() {
    let c = cenario();
    c.planejamento_com_salario(2_000_000);
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert_eq!(r.sugestoes.len(), 2, "{:?}", r.sugestoes);
    assert!(matches!(r.sugestoes[0], Sugestao::Vincular { .. }));
    match &r.sugestoes[1] {
        Sugestao::UsarRubricas { rotulo, uso } => {
            assert_eq!(
                rotulo,
                "Usar as 32 rubricas do mês no Salário e as 19 fixas desde 11/2026"
            );
            let marcacao = uso.eventuais_iniciais();
            let comps = uso.composicoes(&marcacao);
            assert_eq!(comps.len(), 2);
            assert_eq!(comps[0].desde, Competencia { ano: 2026, mes: 10 });
            assert_eq!(comps[1].desde, Competencia { ano: 2026, mes: 11 });
            assert_eq!(uso.liquido_do_mes(), Centavos(LIQUIDO_DE_2026));
            // A divergência compara as fixas com o digitado (R$ 20.000,00).
            let fixas = uso.liquido_das_fixas(&marcacao);
            assert_eq!(
                uso.divergencia(&marcacao),
                Some(Centavos(fixas.0 - 2_000_000))
            );
        }
        outra => panic!("{outra:?}"),
    }
    assert!(conciliar_sugerido(&r).is_none());
    assert!(
        r.avisos.iter().any(|a| a.contains("não entra no total")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn com_o_credito_exato_no_extrato_sugere_conciliar_com_duas_provas() {
    let c = cenario();
    let id = c.planejamento_com_salario(2_000_000);
    // A data impressa no contracheque é 13/10; o crédito caiu em 08/10, em outra conta.
    credito(
        &c,
        "pluggy:txn_sal",
        "2026-10-08",
        LIQUIDO_DE_2026,
        "BANCO FICTICIO · 0001",
        "SALARIO EMPRESA FICTICIA",
    );
    credito(
        &c,
        "pluggy:txn_outro",
        "2026-10-09",
        123_456,
        "BANCO FICTICIO · 0001",
        "PIX RECEBIDO",
    );
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert_eq!(r.sugestoes.len(), 3, "{:?}", r.sugestoes);
    let Some(Sugestao::Conciliar {
        rotulo,
        conciliacao,
        provas,
    }) = conciliar_sugerido(&r)
    else {
        panic!("{:?}", r.sugestoes);
    };
    assert_eq!(
        rotulo,
        "Conciliar o Salário · 10/2026 com o crédito de 08/10 no extrato"
    );
    assert_eq!(conciliacao.movimento, "pluggy:txn_sal");
    assert_eq!(conciliacao.valor, Centavos(LIQUIDO_DE_2026));
    assert_eq!(conciliacao.data.to_string(), "2026-10-08");
    assert_eq!(conciliacao.ocorrencia.recorrente, id);
    assert_eq!(
        conciliacao.ocorrencia.competencia,
        Competencia { ano: 2026, mes: 10 }
    );
    assert_eq!(*provas, 2);
    assert!(
        !r.sugestoes
            .iter()
            .any(|s| s.rotulo().contains("Informar recebido"))
    );
}

#[test]
fn com_par_de_conta_e_assinatura_o_credito_de_valor_diferente_tem_duas_provas_e_o_exato_tres() {
    let c = cenario();
    let id = c.planejamento_com_salario(2_000_000);
    let mut plan = c.plan();
    plan.aplicar(
        Alteracao::MapearConta {
            apelido: "CX-0630".into(),
            conta_do_extrato: Some("BANCO FICTICIO · 0001".into()),
        },
        Autor::Usuario,
    )
    .expect("par");
    let base = plan
        .recorrente(&id)
        .expect("ler")
        .expect("existe")
        .versao_do_registro;
    plan.aplicar(
        Alteracao::DefinirAssinatura {
            recorrente: id.clone(),
            assinatura: Some(sbf_planejamento::Assinatura {
                texto: "salario empresa".into(),
                janela_dias: 10,
            }),
            versao_base: base,
        },
        Autor::Usuario,
    )
    .expect("assinatura");
    drop(plan);
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());

    // Só um crédito de valor diferente (PLR): documento + assinatura/conta = 2 provas.
    credito(
        &c,
        "pluggy:txn_plr",
        "2026-10-08",
        3_100_000,
        "BANCO FICTICIO · 0001",
        "SALARIO EMPRESA FICTICIA",
    );
    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);
    let Some(Sugestao::Conciliar {
        conciliacao,
        provas,
        ..
    }) = conciliar_sugerido(&r)
    else {
        panic!("{:?}", r.sugestoes);
    };
    assert_eq!(conciliacao.movimento, "pluggy:txn_plr");
    assert_eq!(*provas, 2);

    // Com o crédito exato também: o exato (3 provas) vence sozinho.
    credito(
        &c,
        "pluggy:txn_exato",
        "2026-10-09",
        LIQUIDO_DE_2026,
        "BANCO FICTICIO · 0001",
        "SALARIO EMPRESA FICTICIA",
    );
    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);
    let Some(Sugestao::Conciliar {
        conciliacao,
        provas,
        ..
    }) = conciliar_sugerido(&r)
    else {
        panic!("{:?}", r.sugestoes);
    };
    assert_eq!(conciliacao.movimento, "pluggy:txn_exato");
    assert_eq!(*provas, PROVAS_POSSIVEIS);
}

#[test]
fn credito_de_outro_valor_sem_conta_nem_assinatura_nao_sugere() {
    let c = cenario();
    c.planejamento_com_salario(2_000_000);
    credito(
        &c,
        "pluggy:txn_pix",
        "2026-10-08",
        777_700,
        "BANCO FICTICIO · 0001",
        "PIX RECEBIDO",
    );
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert!(conciliar_sugerido(&r).is_none(), "{:?}", r.sugestoes);
}

#[test]
fn dois_creditos_exatos_empatados_nao_sugerem() {
    let c = cenario();
    c.planejamento_com_salario(2_000_000);
    credito(
        &c,
        "pluggy:txn_a",
        "2026-10-07",
        LIQUIDO_DE_2026,
        "BANCO FICTICIO · 0001",
        "TRANSFERENCIA",
    );
    credito(
        &c,
        "pluggy:txn_b",
        "2026-10-08",
        LIQUIDO_DE_2026,
        "BANCO FICTICIO · 0002",
        "TRANSFERENCIA",
    );
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());

    let r = ok(c.rodar(&c.reconhecedor(None), &arquivo).0);

    assert!(conciliar_sugerido(&r).is_none(), "{:?}", r.sugestoes);
}

#[test]
fn confirmar_a_conciliacao_grava_confirmada_e_a_sugestao_nao_volta() {
    let c = cenario();
    let id = c.planejamento_com_salario(2_000_000);
    credito(
        &c,
        "pluggy:txn_sal",
        "2026-10-08",
        LIQUIDO_DE_2026,
        "BANCO FICTICIO · 0001",
        "SALARIO EMPRESA FICTICIA",
    );
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());
    let r = c.reconhecedor(None);

    let primeira = ok(c.rodar(&r, &arquivo).0);
    let s = conciliar_sugerido(&primeira).expect("sugere conciliar");
    let alteracoes = s.alteracoes(&primeira.sha256, &primeira.nome, 0, None);
    assert_eq!(alteracoes.len(), 1);
    assert!(matches!(
        &alteracoes[0],
        Alteracao::Conciliar { conciliacao } if conciliacao.confirmada
    ));
    let mut plan = c.plan();
    for a in alteracoes {
        plan.aplicar(a, Autor::Usuario).expect("aplicar");
    }
    let o = plan
        .proximas(
            &id,
            Competencia { ano: 2026, mes: 10 },
            1,
            sbf_planejamento::Data::nova(2026, 10, 20).expect("data"),
        )
        .expect("proximas");
    assert_eq!(o[0].conciliado, Some(Centavos(LIQUIDO_DE_2026)));
    drop(plan);

    let segunda = ok(c.rodar(&r, &arquivo).0);
    assert!(
        conciliar_sugerido(&segunda).is_none(),
        "{:?}",
        segunda.sugestoes
    );
}

#[test]
fn composicoes_de_2026_confirmadas_nao_sao_sugeridas_de_novo() {
    let c = cenario();
    let id = c.planejamento_com_salario(2_000_000);
    let arquivo = c.solto("contracheque.txt", CONTRACHEQUE_2026.as_bytes());
    let r = c.reconhecedor(None);
    let primeira = ok(c.rodar(&r, &arquivo).0);
    let mut plan = c.plan();
    for s in &primeira.sugestoes {
        for mut a in s.alteracoes(&primeira.sha256, &primeira.nome, 0, None) {
            if let Alteracao::DefinirComposicao { versao_base, .. } = &mut a {
                *versao_base = plan
                    .recorrente(&id)
                    .expect("r")
                    .expect("existe")
                    .versao_do_registro;
            }
            plan.aplicar(a, Autor::Usuario).expect("aplicar");
        }
    }
    drop(plan);

    let segunda = ok(c.rodar(&r, &arquivo).0);

    assert!(segunda.sugestoes.is_empty(), "{:?}", segunda.sugestoes);
}
