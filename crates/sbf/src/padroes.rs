//! A passada de fundo dos padrões de reconhecimento (spec 15, D4): o que o `sbf worker` roda
//! para as listas da janela saberem quais documentos algum padrão já reconheceu.
//!
//! Lê só o texto que o inventário já guarda (nunca extrai, não faz OCR, não chama IA) e roda
//! as versões vigentes (ativas e consolidadas) do `regras.sqlite`, **uma versão por lote**,
//! em lotes pequenos. O ponto de parada de cada versão fica no próprio `regras.sqlite`
//! (chaveado pelo checksum): reiniciar o worker retoma, e uma versão ativada ou alterada tem
//! ponto zero e repete só ela.
//!
//! Grava **só a execução** (quem casou) e a decisão da entrada (o âmbar). Nunca grava
//! proposta, Caixa de entrada, decisão do usuário nem `achado.estado`. Nada de conteúdo de
//! documento vai para log ou erro.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use sbf_reconhecimento::regras::{ARQUIVO_DAS_REGRAS, entrada_do_documento};
use sbf_regras::catalogo::Catalogo;
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Canal, Decisao};
use sbf_worker::laco::{Dados, PassadaDePadroes, ResumoDaPassada};
use sbf_worker::registro::Falha;

/// Quantos documentos um lote lê no máximo.
pub const DOCUMENTOS_POR_LOTE: usize = 25;
/// Quanto um lote pode durar (os scripts das versões podem ser lentos): passou disto, o lote
/// termina no documento atual e o ponto avança só até ele.
const TEMPO_DO_LOTE: Duration = Duration::from_secs(3);

/// A passada real do worker.
pub struct PassadaReal {
    dados: Dados,
    executor: Option<Arc<dyn ExecutorDeScript>>,
    lote: usize,
}

impl std::fmt::Debug for PassadaReal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PassadaReal").finish_non_exhaustive()
    }
}

impl PassadaReal {
    #[must_use]
    pub fn novo(dados: Dados, executor: Option<Arc<dyn ExecutorDeScript>>) -> Self {
        Self {
            dados,
            executor,
            lote: DOCUMENTOS_POR_LOTE,
        }
    }

    /// Outro tamanho de lote (os testes).
    #[cfg(test)]
    #[must_use]
    pub fn com_lote(mut self, n: usize) -> Self {
        self.lote = n.max(1);
        self
    }

    fn catalogo(&self) -> PathBuf {
        self.dados.cofre.join(ARQUIVO_DAS_REGRAS)
    }
}

/// Um documento com texto, pronto para virar [`sbf_regras::Entrada`].
struct Linha {
    ordem: i64,
    sha256: String,
    texto: String,
    mime: Option<String>,
    nome: Option<String>,
    origem: Option<String>,
}

fn canal_da_origem(tipo: Option<&str>) -> Canal {
    match tipo {
        Some("gmail") => Canal::Gmail,
        Some("telegram") => Canal::Telegram,
        _ => Canal::Cofre,
    }
}

fn ler_linhas(inv: &Connection, depois_de: i64, n: usize) -> rusqlite::Result<Vec<Linha>> {
    let mut st = inv.prepare_cached(
        "SELECT t.rowid, t.content_hash, t.conteudo, a.mime_type,
                (SELECT o.filename FROM origem o WHERE o.content_hash = t.content_hash
                  ORDER BY o.id DESC LIMIT 1),
                (SELECT o.origin_type FROM origem o WHERE o.content_hash = t.content_hash
                  ORDER BY o.id LIMIT 1)
           FROM texto t JOIN achado a ON a.content_hash = t.content_hash
          WHERE t.rowid > ?1 AND trim(t.conteudo) <> ''
          ORDER BY t.rowid LIMIT ?2",
    )?;
    let limite = i64::try_from(n).unwrap_or(i64::MAX);
    st.query_map(rusqlite::params![depois_de, limite], |r| {
        Ok(Linha {
            ordem: r.get(0)?,
            sha256: r.get(1)?,
            texto: r.get(2)?,
            mime: r.get(3)?,
            nome: r.get(4)?,
            origem: r.get(5)?,
        })
    })?
    .collect()
}

fn total_de_documentos(inv: &Connection) -> rusqlite::Result<u64> {
    inv.query_row(
        "SELECT COUNT(*) FROM texto WHERE trim(conteudo) <> ''",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| u64::try_from(n).unwrap_or(0))
}

impl PassadaDePadroes for PassadaReal {
    fn lote(&mut self, cancelar: &AtomicBool) -> Result<ResumoDaPassada, Falha> {
        let caminho = self.catalogo();
        // Quem cria o catálogo é o reconhecimento (e a janela): a passada não o inventa.
        if !caminho.exists() {
            return Ok(ResumoDaPassada::default());
        }
        let mut catalogo = Catalogo::abrir(&caminho).map_err(|_| Falha::Banco)?;
        let vigentes = catalogo.vigentes().map_err(|_| Falha::Banco)?;
        let inv = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        let total = total_de_documentos(&inv).map_err(|_| Falha::Banco)?
            * u64::try_from(vigentes.len()).unwrap_or(0);
        let progresso = |c: &Catalogo| c.progresso_da_passada().unwrap_or(0).min(total);

        // Uma versão por lote: a primeira que ainda tem documento depois do ponto dela.
        for versao in &vigentes {
            let ponto = catalogo
                .ponto_da_passada(versao)
                .map_err(|_| Falha::Banco)?;
            let linhas = ler_linhas(&inv, ponto, self.lote).map_err(|_| Falha::Banco)?;
            if linhas.is_empty() {
                continue;
            }
            let parametros = catalogo.parametros().map_err(|_| Falha::Banco)?;
            // O motor desta versão sozinha (a leitura do conjunto inteiro só se ela casar).
            let so_esta = Motor::novo(vec![versao.clone()], parametros, self.executor.clone());
            let mut todas: Option<Motor> = None;
            let comeco = Instant::now();
            let (mut ultimo, mut lidos) = (ponto, 0usize);
            for l in &linhas {
                if cancelar.load(Ordering::Relaxed)
                    || (lidos > 0 && comeco.elapsed() > TEMPO_DO_LOTE)
                {
                    break;
                }
                let entrada = entrada_do_documento(
                    &l.sha256,
                    l.nome.as_deref().unwrap_or_default(),
                    l.mime.as_deref().unwrap_or("application/octet-stream"),
                    Some(&l.texto),
                    canal_da_origem(l.origem.as_deref()),
                    None,
                );
                let leitura = so_esta.ler(&entrada);
                catalogo
                    .registrar_execucao_de_fundo(&leitura)
                    .map_err(|_| Falha::Banco)?;
                if matches!(leitura.decisao, Decisao::Reconhecido(_)) {
                    // Casou: só o conjunto inteiro sabe se é um empate (âmbar).
                    if todas.is_none() {
                        todas = Some(
                            Motor::das_vigentes(&catalogo, self.executor.clone())
                                .map_err(|_| Falha::Banco)?,
                        );
                    }
                    if let Some(m) = &todas {
                        catalogo
                            .registrar_decisao(&m.ler(&entrada))
                            .map_err(|_| Falha::Banco)?;
                    }
                }
                ultimo = l.ordem;
                lidos += 1;
            }
            if lidos > 0 {
                catalogo
                    .avancar_a_passada(versao, ultimo, lidos as u64)
                    .map_err(|_| Falha::Banco)?;
            }
            return Ok(ResumoDaPassada {
                lidos,
                feitos: progresso(&catalogo),
                total,
            });
        }
        Ok(ResumoDaPassada {
            lidos: 0,
            feitos: progresso(&catalogo),
            total,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_inventario::banco::Inventario;
    use sbf_regras::catalogo::NovaVersao;
    use sbf_regras::definicao::Definicao;
    use sbf_regras::marcacao::CorDaMarca;
    use sbf_regras::{Autor, Quem};

    const PIX: &str =
        "Banco Exemplo\nComprovante PIX\nFavorecido: Fulano de Tal\nValor: R$ 10,00\n";

    struct Cenario {
        _raiz: tempfile::TempDir,
        dados: Dados,
    }

    fn cenario() -> Cenario {
        let raiz = tempfile::tempdir().expect("raiz");
        let cofre = raiz.path().join("cofre");
        let pasta = raiz.path().join("dados");
        std::fs::create_dir_all(&cofre).expect("cofre");
        std::fs::create_dir_all(&pasta).expect("dados");
        let mut inv = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
        inv.migrar(raiz.path().join("copia.sqlite"))
            .expect("migrar");
        Cenario {
            dados: Dados::novos(&cofre, &pasta),
            _raiz: raiz,
        }
    }

    fn guardar(c: &Cenario, sha: &str, texto: &str) {
        let conn = sbf_worker::banco::abrir(&c.dados.banco).expect("abrir");
        conn.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada)
             VALUES (?1, 10, 'application/pdf', 'pdf', 'A')",
            [sha],
        )
        .expect("achado");
        conn.execute(
            "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
            [sha, texto],
        )
        .expect("texto");
    }

    fn ativar(c: &Cenario, regra: &str, contem: &str) -> sbf_regras::catalogo::VersaoDeRegra {
        let mut cat = Catalogo::abrir(&c.dados.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catálogo");
        let v = cat
            .salvar_rascunho(NovaVersao {
                regra: regra.to_owned(),
                nome: regra.to_owned(),
                definicao: Definicao::simples(regra, contem),
                script: None,
                autor: Autor::Usuario,
                motivo: "teste".to_owned(),
            })
            .expect("rascunho");
        cat.ativar(&v.regra, v.versao, &Quem::Usuario, None)
            .expect("ativar");
        cat.versao(&v.regra, v.versao).expect("versão")
    }

    fn marca(c: &Cenario, sha: &str) -> CorDaMarca {
        let cat = Catalogo::abrir(&c.dados.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catálogo");
        cat.marcas(&[sha.to_owned()])
            .expect("marcas")
            .get(sha)
            .copied()
            .unwrap_or_default()
            .cor()
    }

    fn contar(c: &Cenario, sql: &str) -> i64 {
        sbf_worker::banco::abrir(&c.dados.banco)
            .expect("abrir")
            .query_row(sql, [], |r| r.get(0))
            .expect("contar")
    }

    fn nao() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn sem_catalogo_a_passada_nao_faz_nada_nem_cria_o_arquivo() {
        let c = cenario();
        guardar(&c, "aaa", PIX);
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        assert_eq!(p.lote(&nao()).expect("lote"), ResumoDaPassada::default());
        assert!(!c.dados.cofre.join(ARQUIVO_DAS_REGRAS).exists());
    }

    #[test]
    fn marca_o_que_casou_em_lotes_pequenos_e_nao_grava_nada_alem_da_execucao() {
        let c = cenario();
        guardar(&c, "doc1", PIX);
        guardar(&c, "doc2", "um texto qualquer");
        guardar(&c, "doc3", PIX);
        guardar(&c, "doc4", "outro texto");
        ativar(&c, "pix", "comprovante pix");
        let estados_antes = contar(&c, "SELECT COUNT(*) FROM achado WHERE estado = 'novo'");

        let mut p = PassadaReal::novo(c.dados.clone(), None).com_lote(3);
        let r1 = p.lote(&nao()).expect("lote 1");
        assert_eq!((r1.lidos, r1.feitos, r1.total), (3, 3, 4));
        let r2 = p.lote(&nao()).expect("lote 2");
        assert_eq!((r2.lidos, r2.feitos, r2.total), (1, 4, 4));
        let r3 = p.lote(&nao()).expect("lote 3");
        assert_eq!((r3.lidos, r3.feitos, r3.total), (0, 4, 4));

        assert_eq!(marca(&c, "doc1"), CorDaMarca::Verde);
        assert_eq!(marca(&c, "doc3"), CorDaMarca::Verde);
        assert_eq!(marca(&c, "doc2"), CorDaMarca::SemCor);
        assert_eq!(marca(&c, "doc4"), CorDaMarca::SemCor);

        // Só a execução: nem Caixa de entrada, nem decisão, nem estado do achado.
        assert_eq!(contar(&c, "SELECT COUNT(*) FROM caixa_entrada"), 0);
        assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
        assert_eq!(
            contar(&c, "SELECT COUNT(*) FROM achado WHERE estado = 'novo'"),
            estados_antes
        );
        let cat = Catalogo::abrir(&c.dados.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catálogo");
        assert!(cat.propostas(None).expect("propostas").is_empty());
    }

    #[test]
    fn retoma_do_ponto_depois_de_reiniciar_e_pega_o_texto_novo() {
        let c = cenario();
        for i in 0..5 {
            guardar(&c, &format!("d{i}"), PIX);
        }
        ativar(&c, "pix", "comprovante pix");
        let mut p = PassadaReal::novo(c.dados.clone(), None).com_lote(2);
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 2);
        // O worker reinicia: uma passada nova retoma, sem refazer os dois primeiros.
        let mut p = PassadaReal::novo(c.dados.clone(), None).com_lote(10);
        let r = p.lote(&nao()).expect("lote");
        assert_eq!((r.lidos, r.feitos), (3, 5));
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 0);
        // Texto que chega depois (o OCR terminou) entra na próxima.
        guardar(&c, "novo", PIX);
        let r = p.lote(&nao()).expect("lote");
        assert_eq!((r.lidos, r.feitos, r.total), (1, 6, 6));
        assert_eq!(marca(&c, "novo"), CorDaMarca::Verde);
    }

    #[test]
    fn versao_nova_repete_so_ela_e_o_empate_vira_ambar() {
        let c = cenario();
        guardar(&c, "doc1", PIX);
        guardar(&c, "doc2", PIX);
        let a = ativar(&c, "regra-a", "comprovante pix");
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 2);
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 0);
        assert_eq!(marca(&c, "doc1"), CorDaMarca::Verde);

        // Uma segunda regra, de mesma prioridade e especificidade, discordando no tipo.
        ativar(&c, "regra-b", "banco exemplo");
        let r = p.lote(&nao()).expect("lote");
        assert_eq!(r.lidos, 2, "só a versão nova é lida");
        assert_eq!(r.total, 4);
        assert_eq!(marca(&c, "doc1"), CorDaMarca::Ambar);
        assert_eq!(marca(&c, "doc2"), CorDaMarca::Ambar);
        // A versão antiga não foi refeita.
        let cat = Catalogo::abrir(&c.dados.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catálogo");
        assert!(cat.ponto_da_passada(&a).expect("ponto") > 0);
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 0);
    }

    #[test]
    fn cancelar_para_antes_do_primeiro_documento() {
        let c = cenario();
        guardar(&c, "doc1", PIX);
        ativar(&c, "pix", "comprovante pix");
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        let r = p.lote(&AtomicBool::new(true)).expect("lote");
        assert_eq!(r.lidos, 0);
        assert_eq!(marca(&c, "doc1"), CorDaMarca::SemCor);
    }
}
