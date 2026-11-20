//! A passada de fundo dos padrões de reconhecimento (spec 15, D4): o que o `sbf worker` roda
//! para as listas da janela saberem quais documentos algum padrão já reconheceu.
//!
//! Lê só o texto que o inventário já guarda (nunca extrai, não faz OCR, não chama IA) e roda
//! as versões vigentes (ativas e consolidadas) do `regras.sqlite` **num lote só**: cada
//! documento é lido **uma vez** com o conjunto das versões que ainda não o viram. O ponto de
//! parada de cada versão fica no próprio `regras.sqlite` (chaveado pelo checksum): reiniciar o
//! worker retoma, e uma versão ativada ou alterada tem ponto zero e repete só ela (os
//! documentos que as outras já viram rodam só a versão nova).
//!
//! Grava **só a execução** (quem casou) e a decisão da entrada (o âmbar). Nunca grava
//! proposta, Caixa de entrada, decisão do usuário nem `achado.estado`. Nada de conteúdo de
//! documento vai para log ou erro.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use sbf_reconhecimento::regras::{ARQUIVO_DAS_REGRAS, entrada_do_documento};
use sbf_regras::catalogo::{Catalogo, VersaoDeRegra};
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Canal, Decisao};
use sbf_worker::laco::{Dados, PassadaDePadroes, ResumoDaPassada};
use sbf_worker::registro::Falha;

/// Quantos documentos um lote lê no máximo (cada um, uma vez, com todas as versões pendentes).
pub const DOCUMENTOS_POR_LOTE: usize = 200;
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

/// Grava o que uma leitura diz de um documento e conta quem a leu.
struct Lote<'a> {
    catalogo: &'a mut Catalogo,
    vigentes: &'a [VersaoDeRegra],
    /// O ponto de cada versão no começo do lote.
    pontos: Vec<i64>,
    completo: Motor,
    /// Os motores de subconjuntos de versões (os que ainda não leram o documento), por
    /// índices; montados uma vez por lote.
    parciais: HashMap<Vec<usize>, Motor>,
    parametros: Vec<sbf_regras::catalogo::Parametro>,
    executor: Option<Arc<dyn ExecutorDeScript>>,
    /// Quantos documentos cada versão leu neste lote.
    feitos: Vec<u64>,
}

impl Lote<'_> {
    fn ler(&mut self, l: &Linha) -> Result<(), Falha> {
        let entrada = entrada_do_documento(
            &l.sha256,
            l.nome.as_deref().unwrap_or_default(),
            l.mime.as_deref().unwrap_or("application/octet-stream"),
            Some(&l.texto),
            canal_da_origem(l.origem.as_deref()),
            None,
        );
        let pendentes: Vec<usize> = (0..self.vigentes.len())
            .filter(|&i| self.pontos[i] < l.ordem)
            .collect();
        if pendentes.is_empty() {
            return Ok(());
        }
        if pendentes.len() == self.vigentes.len() {
            // Todas as versões ainda não leram este documento: uma leitura do conjunto inteiro
            // dá as execuções e a decisão (o âmbar) de uma vez.
            let leitura = self.completo.ler(&entrada);
            self.catalogo
                .registrar_execucao_de_fundo(&leitura)
                .map_err(|_| Falha::Banco)?;
            self.catalogo
                .registrar_decisao(&leitura)
                .map_err(|_| Falha::Banco)?;
        } else {
            // Só as versões novas leem; a decisão do conjunto só é refeita se alguma casou.
            let (vigentes, parametros, executor) =
                (self.vigentes, &self.parametros, &self.executor);
            let motor = self.parciais.entry(pendentes.clone()).or_insert_with(|| {
                Motor::novo(
                    pendentes.iter().map(|&i| vigentes[i].clone()).collect(),
                    parametros.clone(),
                    executor.clone(),
                )
            });
            let leitura = motor.ler(&entrada);
            self.catalogo
                .registrar_execucao_de_fundo(&leitura)
                .map_err(|_| Falha::Banco)?;
            if matches!(leitura.decisao, Decisao::Reconhecido(_)) {
                // Casou: só o conjunto inteiro sabe se é um empate (âmbar).
                self.catalogo
                    .registrar_decisao(&self.completo.ler(&entrada))
                    .map_err(|_| Falha::Banco)?;
            }
        }
        for i in pendentes {
            self.feitos[i] += 1;
        }
        Ok(())
    }
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
        let sem_trabalho = |c: &Catalogo| ResumoDaPassada {
            lidos: 0,
            feitos: progresso(c),
            total,
        };
        if vigentes.is_empty() {
            return Ok(sem_trabalho(&catalogo));
        }

        // O lote começa onde a versão mais atrasada parou.
        let pontos = vigentes
            .iter()
            .map(|v| catalogo.ponto_da_passada(v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Falha::Banco)?;
        let comeco_do_lote = pontos.iter().copied().min().unwrap_or(0);
        let linhas = ler_linhas(&inv, comeco_do_lote, self.lote).map_err(|_| Falha::Banco)?;
        if linhas.is_empty() {
            return Ok(sem_trabalho(&catalogo));
        }
        let parametros = catalogo.parametros().map_err(|_| Falha::Banco)?;
        let completo = Motor::novo(vigentes.clone(), parametros.clone(), self.executor.clone());
        let mut lote = Lote {
            catalogo: &mut catalogo,
            vigentes: &vigentes,
            feitos: vec![0; vigentes.len()],
            pontos,
            completo,
            parciais: HashMap::new(),
            parametros,
            executor: self.executor.clone(),
        };
        let comeco = Instant::now();
        let (mut ultimo, mut lidos) = (comeco_do_lote, 0usize);
        for l in &linhas {
            if cancelar.load(Ordering::Relaxed) || (lidos > 0 && comeco.elapsed() > TEMPO_DO_LOTE) {
                break;
            }
            lote.ler(l)?;
            ultimo = l.ordem;
            lidos += 1;
        }
        let feitos = lote.feitos;
        for (versao, n) in vigentes.iter().zip(feitos) {
            if n > 0 {
                catalogo
                    .avancar_a_passada(versao, ultimo, n)
                    .map_err(|_| Falha::Banco)?;
            }
        }
        Ok(ResumoDaPassada {
            lidos,
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

    #[test]
    fn cada_documento_e_lido_uma_vez_com_todas_as_versoes_no_mesmo_lote() {
        let c = cenario();
        for i in 0..3 {
            guardar(&c, &format!("d{i}"), PIX);
        }
        ativar(&c, "a", "comprovante pix");
        ativar(&c, "b", "banco exemplo");
        ativar(&c, "c", "favorecido");
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        // Um lote só cobre os 3 documentos das 3 versões (antes eram 3 lotes, um por versão).
        let r = p.lote(&nao()).expect("lote");
        assert_eq!((r.lidos, r.feitos, r.total), (3, 9, 9));
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 0);
        for i in 0..3 {
            assert_ne!(marca(&c, &format!("d{i}")), CorDaMarca::SemCor);
        }
        // O tamanho do lote é o teto de documentos por volta.
        assert_eq!(DOCUMENTOS_POR_LOTE, 200);
    }

    #[test]
    fn versao_nova_com_outras_ja_lidas_roda_so_ela_sem_refazer_as_outras() {
        let c = cenario();
        for i in 0..4 {
            guardar(&c, &format!("d{i}"), PIX);
        }
        ativar(&c, "a", "comprovante pix");
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        assert_eq!(p.lote(&nao()).expect("lote").feitos, 4);
        let antes = contar(&c, "SELECT 1");
        assert_eq!(antes, 1);
        let execucoes = |c: &Cenario| {
            let cat = Catalogo::abrir(&c.dados.cofre.join(ARQUIVO_DAS_REGRAS)).expect("catálogo");
            cat.total_de_execucoes().expect("total")
        };
        assert_eq!(execucoes(&c), 4);
        ativar(&c, "b", "banco exemplo");
        let r = p.lote(&nao()).expect("lote");
        assert_eq!((r.lidos, r.feitos, r.total), (4, 8, 8));
        // Só 4 execuções novas (a versão nova); as 4 da primeira não foram refeitas.
        assert_eq!(execucoes(&c), 8);
        assert_eq!(p.lote(&nao()).expect("lote").lidos, 0);
    }

    /// Mede a passada sobre um cofre fictício: `cargo test -p sbf --bin sbf medida_da_passada --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "medida de ritmo: roda sob demanda"]
    fn medida_da_passada_em_documentos_por_segundo() {
        const DOCUMENTOS: usize = 3000;
        let c = cenario();
        {
            let conn = sbf_worker::banco::abrir(&c.dados.banco).expect("abrir");
            conn.execute_batch("BEGIN").expect("transação");
            for i in 0..DOCUMENTOS {
                let sha = format!("doc{i:06}");
                let texto = if i % 3 == 0 {
                    format!(
                        "Banco Exemplo\nComprovante PIX\nFavorecido: Pessoa {i}\nValor: R$ {i},00\n"
                    )
                } else {
                    format!(
                        "Fatura de energia {i}\nVencimento: 10/11/2026\nValor: R$ {i},00\nContrato {i}\n"
                    )
                };
                conn.execute(
                    "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada)
                     VALUES (?1, 10, 'application/pdf', 'pdf', 'A')",
                    [&sha],
                )
                .expect("achado");
                conn.execute(
                    "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
                    [sha, texto],
                )
                .expect("texto");
            }
            conn.execute_batch("COMMIT").expect("commit");
        }
        for (regra, contem) in [
            ("pix", "comprovante pix"),
            ("banco", "banco exemplo"),
            ("energia", "fatura de energia"),
            ("vencimento", "vencimento"),
            ("valor", "valor"),
            ("contrato", "contrato"),
        ] {
            ativar(&c, regra, contem);
        }
        let mut p = PassadaReal::novo(c.dados.clone(), None);
        let t0 = Instant::now();
        let (mut lotes, mut lidos) = (0, 0);
        loop {
            let r = p.lote(&nao()).expect("lote");
            if r.lidos == 0 {
                break;
            }
            lotes += 1;
            lidos += r.lidos;
        }
        let seg = t0.elapsed().as_secs_f64();
        eprintln!(
            "passada: {lidos} documentos x 6 versões em {seg:.2}s = {:.0} docs/s ({:.0} leituras/s), {lotes} lotes de até {DOCUMENTOS_POR_LOTE}",
            lidos as f64 / seg,
            lidos as f64 * 6.0 / seg
        );
        assert_eq!(lidos, DOCUMENTOS);
    }
}
