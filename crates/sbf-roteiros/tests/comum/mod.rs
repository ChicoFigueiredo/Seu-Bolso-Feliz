//! Apoio dos testes: o site falso em memória, as senhas fictícias e o inventário em arquivo.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sbf_roteiros::executor::{Decisao, FonteDeSenhas, Pausa};
use sbf_roteiros::falso::{Elemento, NavegadorFalso};
use sbf_roteiros::pagina::Elemento as ElementoDePagina;
use sbf_roteiros::pagina::{Campo, Formulario, Pagina};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SENHA: &str = "SenhaFicticia#4821";
pub const CPF: &str = "123.456.789-09";
const SQL_BASE: &str = include_str!("../../../sbf-inventario/sql/0001-base-bun-v2.sql");

#[derive(Debug)]
pub struct Senhas;

impl FonteDeSenhas for Senhas {
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String> {
        if rotulo == "banco-ficticio" {
            Ok(Segredo::novo(SENHA))
        } else {
            Err(format!("a senha do site \"{rotulo}\" não está no chaveiro"))
        }
    }
    fn cpf(&self) -> Result<Segredo, String> {
        Ok(Segredo::novo(CPF))
    }
}

/// Uma pausa que responde na ordem e anota os motivos.
#[derive(Debug, Default)]
pub struct PausaFalsa {
    pub respostas: Mutex<VecDeque<Decisao>>,
    pub motivos: Mutex<Vec<String>>,
    /// Acende quando o usuário "continua" (o site falso libera o painel).
    pub sinal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl PausaFalsa {
    pub fn com(respostas: &[Decisao]) -> Self {
        Self {
            respostas: Mutex::new(respostas.iter().copied().collect()),
            motivos: Mutex::default(),
            sinal: None,
        }
    }
}

impl Pausa for PausaFalsa {
    fn aguardar(&self, _execucao: i64, motivo: &str) -> Decisao {
        self.motivos.lock().unwrap().push(motivo.to_owned());
        let d = self
            .respostas
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Decisao::Cancelar);
        if d == Decisao::Continuar
            && let Some(s) = &self.sinal
        {
            s.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        d
    }
}

pub fn clicavel(seletor: &str, texto: &str) -> ElementoDePagina {
    ElementoDePagina {
        seletor: seletor.into(),
        tag: "button".into(),
        texto: texto.into(),
        ..ElementoDePagina::default()
    }
}

/// O site: login → (MFA) → painel com extrato, tabela e o botão Pagar.
pub fn site(com_mfa: bool) -> NavegadorFalso {
    let mut n = NavegadorFalso::default();
    n.rotas
        .insert("http://127.0.0.1:1/login".into(), "login".into());
    n.pagina_nomeada(
        "login",
        Pagina {
            url: "http://127.0.0.1:1/login".into(),
            titulo: "Entrar".into(),
            texto: "Acesse sua conta".into(),
            elementos: vec![clicavel("#entrar", "Entrar")],
            formularios: vec![Formulario {
                seletor: "#f".into(),
                acao: "/login".into(),
                metodo: "post".into(),
                campos: vec![
                    Campo {
                        seletor: "#cpf".into(),
                        tipo: "text".into(),
                        ..Campo::default()
                    },
                    Campo {
                        seletor: "#senha".into(),
                        tipo: "password".into(),
                        ..Campo::default()
                    },
                ],
            }],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "mfa",
        Pagina {
            titulo: "Verificação".into(),
            texto: "Digite o código de verificação enviado por SMS".into(),
            elementos: vec![clicavel("#confirmar-codigo", "Validar")],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "painel",
        Pagina {
            titulo: "Painel".into(),
            texto: "Seus extratos".into(),
            elementos: vec![
                clicavel("#baixar", "Baixar extrato"),
                clicavel("#pagar", "Pagar"),
                clicavel("#ver", "Continuar"),
            ],
            tabelas: vec![sbf_roteiros::pagina::Tabela {
                seletor: "#extratos".into(),
                linhas: 2,
                cabecalho: vec!["Mês".into(), "Valor".into()],
            }],
            ..Pagina::default()
        },
    );
    n.elementos.insert("#cpf".into(), {
        let mut e = Elemento::com_texto("");
        e.candidato.eh_campo = true;
        e
    });
    n.elementos.insert("#senha".into(), {
        let mut e = Elemento::com_texto("");
        e.candidato.eh_campo = true;
        e.candidato.eh_senha = true;
        e
    });
    let mut entrar = Elemento::com_texto("Entrar");
    entrar.leva_a = Some(if com_mfa { "mfa" } else { "painel" }.into());
    n.elementos.insert("#entrar".into(), entrar);
    let mut valida = Elemento::com_texto("Validar");
    valida.leva_a = Some("painel".into());
    n.elementos.insert("#confirmar-codigo".into(), valida);
    let mut baixar = Elemento::com_texto("Baixar extrato");
    baixar.baixa = Some((
        "extrato-2026-10.pdf".into(),
        b"%PDF-1.4 extrato ficticio outubro".to_vec(),
    ));
    n.elementos.insert("#baixar".into(), baixar);
    n.elementos
        .insert("#pagar".into(), Elemento::com_texto("Pagar"));
    n.elementos
        .insert("#ver".into(), Elemento::com_texto("Continuar"));
    n.tabelas.insert(
        "#extratos".into(),
        vec![
            vec!["Mês".into(), "Valor".into()],
            vec!["Outubro".into(), "R$ 1.234,56".into()],
        ],
    );
    n
}

/// Cofre e inventário em arquivo, com o esquema da base.
pub fn inventario(pasta: &Path) -> (PathBuf, Connection) {
    let cofre = pasta.join("cofre");
    std::fs::create_dir_all(&cofre).unwrap();
    let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
    conn.execute_batch(SQL_BASE).unwrap();
    (cofre, conn)
}
