//! INV-09 sobre o inventário: o PDF de teste abre pela regra, o texto sai igual ao do
//! `pdftotext` sobre o original decifrado, a senha não vai para o banco, N falhas viram
//! pendência, e a regra só é gravada com confirmação.

mod comum;
#[path = "../../sbf-inventario/tests/nucleo_comum/mod.rs"]
mod nucleo_comum;

use comum::*;
use rusqlite::{Connection, params};
use sbf_inventario::comando::{Acao, Comandos};
use sbf_inventario::protegidos::{TIPO_CORROMPIDO, TIPO_SEM_REGRA};
use sbf_inventario::{Ref, Tipo};
use sbf_pdf::{ErroDeFonte, FonteDeSegredos, OpcoesDaRodada, Situacao, abrir_protegidos};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const HASH: &str = "5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";

struct Ficticia;

impl FonteDeSegredos for Ficticia {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new(CPF.to_owned()))
    }
    fn senha_propria(&self, rotulo: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        match rotulo {
            "fatura-alfa" => Ok(Zeroizing::new(SENHA_PROPRIA.to_owned())),
            _ => Err(ErroDeFonte::SemSenhaPropria),
        }
    }
}

struct Cenario {
    pasta: tempfile::TempDir,
    inventario: sbf_inventario::banco::Inventario,
    pdf: PathBuf,
    referencia: String,
}

impl Cenario {
    /// Um cofre fictício com `acervo/fatura.pdf` cifrado com `senha`, e o achado dele.
    fn novo(senha: &str) -> Self {
        let pasta = pasta();
        let inventario = nucleo_comum::inventario_migrado(pasta.path());
        let base = pdf_ficticio();
        std::fs::create_dir_all(pasta.path().join("acervo")).expect("acervo");
        let pdf = cifrar(
            &pasta.path().join("acervo"),
            "fatura",
            &base,
            senha,
            Cifra::Aes256,
        );
        let referencia = pdftotext_de(&pasta.path().join("acervo").join("fatura-base.pdf"));
        std::fs::remove_file(pasta.path().join("acervo").join("fatura-base.pdf")).expect("limpar");
        inventario
            .conexao()
            .execute(
                "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, document_type,
                                     supplier_name_raw, protegido_senha, caminho_acervo)
                 VALUES (?1, 10, 'A', 'documento', 'fatura', 'Banco Alfa', 1, 'acervo/fatura.pdf')",
                [HASH],
            )
            .expect("achado");
        Self {
            pasta,
            inventario,
            pdf,
            referencia,
        }
    }

    fn c(&self) -> &Connection {
        self.inventario.conexao()
    }

    fn regra(&self, tipo: &str, estrategia: &str, prioridade: i64) -> i64 {
        self.c()
            .execute(
                "INSERT INTO regra_senha (fornecedor, document_type, estrategia, prioridade)
                 VALUES ('Banco Alfa', ?1, ?2, ?3)",
                params![tipo, estrategia, prioridade],
            )
            .expect("regra");
        self.c().last_insert_rowid()
    }

    fn rodar(&self, opcoes: &OpcoesDaRodada) -> sbf_pdf::RelatorioDaRodada {
        abrir_protegidos(self.c(), self.pasta.path(), &Ficticia, opcoes).expect("rodada")
    }

    fn tentativas(&self) -> Vec<(String, i64)> {
        self.c()
            .prepare("SELECT estrategia, sucesso FROM tentativa_senha WHERE content_hash = ?1 ORDER BY id")
            .expect("sql")
            .query_map([HASH], |l| Ok((l.get(0)?, l.get(1)?)))
            .expect("tentativas")
            .collect::<Result<_, _>>()
            .expect("linhas")
    }

    fn pendencias(&self) -> Vec<String> {
        self.c()
            .prepare("SELECT tipo FROM pendencia WHERE referencia = ?1 AND resolvida_em IS NULL")
            .expect("sql")
            .query_map([format!("achado:{HASH}")], |l| l.get(0))
            .expect("pendências")
            .collect::<Result<_, _>>()
            .expect("linhas")
    }

    fn texto(&self) -> Option<(String, String)> {
        self.c()
            .query_row(
                "SELECT metodo, conteudo FROM texto WHERE content_hash = ?1",
                [HASH],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .ok()
    }

    fn contar(&self, sql: &str) -> i64 {
        self.c().query_row(sql, [], |l| l.get(0)).expect("contar")
    }
}

fn todos_os_arquivos(pasta: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    for e in std::fs::read_dir(pasta).expect("listar").flatten() {
        let p = e.path();
        if p.is_dir() {
            saida.extend(todos_os_arquivos(&p));
        } else {
            saida.push(p);
        }
    }
    saida.sort();
    saida
}

#[test]
fn abre_pela_regra_com_o_texto_do_pdftotext_e_sem_senha_no_banco() {
    let cenario = Cenario::novo(SENHA_PROPRIA);
    let regra = cenario.regra("fatura", "senha_propria:fatura-alfa", 100);
    let original = std::fs::read(&cenario.pdf).expect("ler");
    let arquivos_antes = todos_os_arquivos(&cenario.pasta.path().join("acervo"));

    let relatorio = cenario.rodar(&OpcoesDaRodada::default());
    assert_eq!(relatorio.resultados.len(), 1);
    assert_eq!(
        relatorio.resultados[0].situacao,
        Situacao::Aberto {
            estrategia: Some("senha_propria:fatura-alfa".into())
        }
    );
    assert_eq!(
        relatorio.propostas().count(),
        0,
        "havia regra: nada a propor"
    );

    let (metodo, conteudo) = cenario.texto().expect("texto");
    assert_eq!(metodo, "pdftotext");
    assert_eq!(
        conteudo, cenario.referencia,
        "texto diferente do pdftotext sobre o original"
    );
    let impressao: String = cenario
        .c()
        .query_row(
            "SELECT canonical_fingerprint FROM achado WHERE content_hash = ?1",
            [HASH],
            |l| l.get(0),
        )
        .expect("impressao");
    assert_eq!(impressao, sbf_pdf::impressao_canonica(&cenario.referencia));
    assert_eq!(
        cenario.tentativas(),
        [("senha_propria:fatura-alfa".to_owned(), 1)]
    );
    let (sucessos, ultimo): (i64, Option<String>) = cenario
        .c()
        .query_row(
            "SELECT sucessos, ultimo_sucesso_em FROM regra_senha WHERE id = ?1",
            [regra],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .expect("regra");
    assert_eq!(sucessos, 1);
    assert!(ultimo.is_some());

    // O original não mudou e nenhum arquivo novo apareceu no acervo.
    assert_eq!(std::fs::read(&cenario.pdf).expect("ler"), original);
    assert_eq!(
        todos_os_arquivos(&cenario.pasta.path().join("acervo")),
        arquivos_antes
    );

    // A senha não está em nenhum byte do banco (nem no WAL) nem em nenhum arquivo da pasta.
    cenario
        .c()
        .execute_batch("PRAGMA wal_checkpoint(FULL);")
        .expect("checkpoint");
    for arquivo in todos_os_arquivos(cenario.pasta.path()) {
        let bytes = std::fs::read(&arquivo).expect("ler");
        assert!(
            !bytes
                .windows(SENHA_PROPRIA.len())
                .any(|j| j == SENHA_PROPRIA.as_bytes()),
            "senha em {}",
            arquivo.display()
        );
    }

    // Segunda rodada: não é mais alvo.
    assert!(
        cenario
            .rodar(&OpcoesDaRodada::default())
            .resultados
            .is_empty()
    );
}

#[test]
fn tenta_as_regras_do_par_por_prioridade_antes_das_do_fornecedor() {
    let cenario = Cenario::novo(CPF_ULTIMOS_5);
    let estrela = cenario.regra("*", "cpf_completo", 1);
    let primeira = cenario.regra("fatura", "cpf_primeiros:3", 50);
    let segunda = cenario.regra("fatura", "cpf_ultimos:5", 60);
    let relatorio = cenario.rodar(&OpcoesDaRodada::default());
    assert_eq!(
        relatorio.resultados[0].situacao,
        Situacao::Aberto {
            estrategia: Some("cpf_ultimos:5".into())
        }
    );
    assert_eq!(
        cenario.tentativas(),
        [
            ("cpf_primeiros:3".to_owned(), 0),
            ("cpf_ultimos:5".to_owned(), 1)
        ]
    );
    let contadores = |id: i64| -> (i64, i64) {
        cenario
            .c()
            .query_row(
                "SELECT sucessos, falhas FROM regra_senha WHERE id = ?1",
                [id],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .expect("regra")
    };
    assert_eq!(contadores(primeira), (0, 1));
    assert_eq!(contadores(segunda), (1, 0));
    assert_eq!(contadores(estrela), (0, 0), "a do `*` nem foi tentada");
}

#[test]
fn n_falhas_viram_pendencia_e_as_tentativas_param() {
    let cenario = Cenario::novo(CPF_ULTIMOS_5);
    cenario.regra("fatura", "cpf_completo", 1);
    cenario.regra("fatura", "cpf_pontuado", 2);
    cenario.regra("fatura", "cpf_primeiros:5", 3);
    let opcoes = OpcoesDaRodada {
        limite_de_falhas: 2,
        ..OpcoesDaRodada::default()
    };
    let relatorio = cenario.rodar(&opcoes);
    assert!(
        matches!(&relatorio.resultados[0].situacao, Situacao::Pendencia { tipo, .. } if tipo == TIPO_SEM_REGRA),
        "{:?}",
        relatorio.resultados[0].situacao
    );
    assert_eq!(cenario.tentativas().len(), 2, "parou no limite");
    assert_eq!(cenario.pendencias(), [TIPO_SEM_REGRA]);
    assert!(cenario.texto().is_none());
    // As tentativas param.
    assert!(cenario.rodar(&opcoes).resultados.is_empty());
    assert_eq!(cenario.tentativas().len(), 2);
}

#[test]
fn sem_regra_e_sem_a_opcao_padrao_nada_e_tentado() {
    let cenario = Cenario::novo(CPF_ULTIMOS_5);
    let relatorio = cenario.rodar(&OpcoesDaRodada::default());
    assert!(matches!(
        relatorio.resultados[0].situacao,
        Situacao::Pendencia { .. }
    ));
    assert!(cenario.tentativas().is_empty(), "nada de tentativa cega");
    assert_eq!(cenario.pendencias(), [TIPO_SEM_REGRA]);
}

#[test]
fn estrategia_padrao_vencedora_vira_proposta_e_so_grava_com_confirmacao() {
    let mut cenario = Cenario::novo(CPF_ULTIMOS_5);
    let opcoes = OpcoesDaRodada {
        tentar_estrategias_padrao: true,
        ..OpcoesDaRodada::default()
    };
    let relatorio = cenario.rodar(&opcoes);
    assert_eq!(
        relatorio.resultados[0].situacao,
        Situacao::Aberto {
            estrategia: Some("cpf_ultimos:5".into())
        }
    );
    let proposta = relatorio.propostas().next().expect("proposta").clone();
    assert_eq!(
        proposta.frase,
        "Faturas do Banco Alfa usam os 5 últimos dígitos do CPF. Guardar como regra?"
    );
    // Sem confirmação, nenhuma regra e nenhuma decisão.
    assert_eq!(cenario.contar("SELECT count(*) FROM regra_senha"), 0);
    assert_eq!(
        cenario.contar("SELECT count(*) FROM decisao WHERE campo = 'regra_senha'"),
        0
    );

    // O usuário confirma: a janela executa o pedido, que vira decisão e regra.
    let pedido = proposta.pedido();
    assert_eq!(
        pedido.acao,
        Acao::GuardarRegraDeSenha {
            document_type: "fatura".into(),
            estrategia: "cpf_ultimos:5".into()
        }
    );
    assert_eq!(pedido.alvos, [Ref::nova(Tipo::Fornecedor, "Banco Alfa")]);
    cenario.inventario.executar(&pedido).expect("confirmar");
    let (fornecedor, tipo, estrategia, ativa): (String, String, String, i64) = cenario
        .c()
        .query_row(
            "SELECT fornecedor, document_type, estrategia, ativa FROM regra_senha",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
        )
        .expect("regra");
    assert_eq!(
        (
            fornecedor.as_str(),
            tipo.as_str(),
            estrategia.as_str(),
            ativa
        ),
        ("Banco Alfa", "fatura", "cpf_ultimos:5", 1)
    );
    assert_eq!(
        cenario.contar("SELECT count(*) FROM decisao WHERE campo = 'regra_senha'"),
        1
    );
}

#[test]
fn pdf_corrompido_vira_pendencia_propria_sem_tentativa() {
    let cenario = Cenario::novo(CPF_ULTIMOS_5);
    cenario.regra("fatura", "cpf_ultimos:5", 1);
    std::fs::write(&cenario.pdf, b"%PDF-1.7\nlixo ficticio\n%%EOF\n").expect("corromper");
    let relatorio = cenario.rodar(&OpcoesDaRodada::default());
    assert!(
        matches!(&relatorio.resultados[0].situacao, Situacao::Pendencia { tipo, .. } if tipo == TIPO_CORROMPIDO)
    );
    assert!(cenario.tentativas().is_empty());
    assert_eq!(cenario.pendencias(), [TIPO_CORROMPIDO]);
}

#[test]
fn chaveiro_trancado_nao_vira_falha_nem_pendencia() {
    struct Trancada;
    impl FonteDeSegredos for Trancada {
        fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
            Err(ErroDeFonte::Trancado)
        }
        fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
            Err(ErroDeFonte::Trancado)
        }
    }
    let cenario = Cenario::novo(CPF_ULTIMOS_5);
    cenario.regra("fatura", "cpf_ultimos:5", 1);
    let relatorio = abrir_protegidos(
        cenario.c(),
        cenario.pasta.path(),
        &Trancada,
        &OpcoesDaRodada::default(),
    )
    .expect("rodada");
    assert_eq!(
        relatorio.resultados[0].situacao,
        Situacao::FaltouSegredo(ErroDeFonte::Trancado)
    );
    assert!(cenario.tentativas().is_empty());
    assert!(cenario.pendencias().is_empty());
}
