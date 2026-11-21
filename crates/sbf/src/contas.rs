//! `sbf planejamento contas`: o código, o par e o apelido de cada conta do planejamento.
//!
//! ```text
//! sbf planejamento contas --inventario <cofre> listar
//! sbf planejamento contas --inventario <cofre> ligar <codigo> <conta-do-extrato> [--autor X]
//! sbf planejamento contas --inventario <cofre> desligar <codigo> [--autor X]
//! sbf planejamento contas --inventario <cofre> apelido <codigo> (<texto> | --limpar) [--autor X]
//! ```
//!
//! Toda escrita é a mesma [`Alteracao`] que a janela aplica (`MapearConta`,
//! `DefinirApelidoDaConta`): fica no histórico, com autor, e tem Desfazer. O autor vale
//! `usuario` (padrão), `importacao`, `mcp`, `ia:<modelo>` ou `regra:<nome>`. A saída mostra
//! contas e rótulos, nunca segredos.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sbf_planejamento::{Alteracao, Autor, Planejamento, Rotulador};

pub const USO: &str = "uso: sbf planejamento contas --inventario <cofre> listar | ligar <codigo> <conta-do-extrato> [--autor X] | desligar <codigo> [--autor X] | apelido <codigo> (<texto> | --limpar) [--autor X]";

type Abrir<'a> = &'a dyn Fn(&Path) -> Result<Box<dyn Planejamento>, String>;

#[derive(Debug, PartialEq, Eq)]
enum Comando {
    Listar,
    Ligar {
        codigo: String,
        conta: String,
    },
    Desligar {
        codigo: String,
    },
    Apelido {
        codigo: String,
        texto: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct Pedido {
    cofre: PathBuf,
    autor: Autor,
    comando: Comando,
}

/// `ia:<modelo>`, `regra:<nome>`, `usuario`, `importacao` ou `mcp`.
fn autor_de(texto: &str) -> Result<Autor, String> {
    let texto = texto.trim();
    match texto.split_once(':') {
        Some(("ia", modelo)) if !modelo.trim().is_empty() => Ok(Autor::Ia {
            modelo: modelo.trim().to_owned(),
        }),
        Some(("regra", regra)) if !regra.trim().is_empty() => Ok(Autor::Regra {
            regra: regra.trim().to_owned(),
        }),
        None => match texto {
            "usuario" => Ok(Autor::Usuario),
            "importacao" => Ok(Autor::Importacao),
            "mcp" => Ok(Autor::Mcp),
            _ => Err(format!("autor desconhecido: {texto}")),
        },
        Some(_) => Err(format!("autor desconhecido: {texto}")),
    }
}

fn ler_pedido(args: &[String]) -> Result<Pedido, String> {
    let mut cofre = None;
    let mut autor = Autor::Usuario;
    let mut limpar = false;
    let mut posicionais: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--inventario" => {
                cofre = Some(PathBuf::from(
                    it.next().ok_or("falta o valor de --inventario")?,
                ));
            }
            "--autor" => autor = autor_de(it.next().ok_or("falta o valor de --autor")?)?,
            "--limpar" => limpar = true,
            outro if outro.starts_with("--") => return Err(format!("opção desconhecida: {outro}")),
            texto => posicionais.push(texto),
        }
    }
    let cofre = cofre.ok_or("falta --inventario")?;
    let comando = match posicionais.as_slice() {
        ["listar"] => Comando::Listar,
        ["ligar", codigo, conta] => Comando::Ligar {
            codigo: (*codigo).to_owned(),
            conta: (*conta).to_owned(),
        },
        ["desligar", codigo] => Comando::Desligar {
            codigo: (*codigo).to_owned(),
        },
        ["apelido", codigo, texto] if !limpar => Comando::Apelido {
            codigo: (*codigo).to_owned(),
            texto: Some((*texto).to_owned()),
        },
        ["apelido", codigo] if limpar => Comando::Apelido {
            codigo: (*codigo).to_owned(),
            texto: None,
        },
        ["apelido", _, _] => return Err("use <texto> ou --limpar, não os dois".to_owned()),
        ["apelido", _] => return Err("falta o texto do apelido (ou --limpar)".to_owned()),
        _ => return Err("comando de contas desconhecido".to_owned()),
    };
    Ok(Pedido {
        cofre,
        autor,
        comando,
    })
}

pub fn rodar(args: &[String], abrir: Abrir<'_>, saida: &mut dyn Write) -> ExitCode {
    let pedido = match ler_pedido(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sbf: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    match executar(&pedido, abrir, saida) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(1)
        }
    }
}

fn executar(p: &Pedido, abrir: Abrir<'_>, saida: &mut dyn Write) -> Result<(), String> {
    let mut plan = abrir(&p.cofre)?;
    match &p.comando {
        Comando::Listar => listar(plan.as_ref(), saida),
        Comando::Ligar { codigo, conta } => {
            let no_extrato = plan.contas_do_extrato().map_err(|e| e.to_string())?;
            if !no_extrato.iter().any(|c| c == conta.trim()) {
                eprintln!(
                    "sbf: atenção: {} não aparece no extrato recente; o par vale mesmo assim",
                    conta.trim()
                );
            }
            aplicar(
                plan.as_mut(),
                Alteracao::MapearConta {
                    codigo: codigo.clone(),
                    conta_do_extrato: Some(conta.clone()),
                },
                &p.autor,
                saida,
            )
        }
        Comando::Desligar { codigo } => aplicar(
            plan.as_mut(),
            Alteracao::MapearConta {
                codigo: codigo.clone(),
                conta_do_extrato: None,
            },
            &p.autor,
            saida,
        ),
        Comando::Apelido { codigo, texto } => aplicar(
            plan.as_mut(),
            Alteracao::DefinirApelidoDaConta {
                codigo: codigo.clone(),
                apelido: texto.clone(),
            },
            &p.autor,
            saida,
        ),
    }
}

fn aplicar(
    plan: &mut dyn Planejamento,
    alteracao: Alteracao,
    autor: &Autor,
    saida: &mut dyn Write,
) -> Result<(), String> {
    let recibo = plan
        .aplicar(alteracao, autor.clone())
        .map_err(|e| e.to_string())?;
    writeln!(
        saida,
        "ok: {} (alteração {})",
        recibo.alvo, recibo.alteracao_id
    )
    .map_err(|e| e.to_string())
}

fn listar(plan: &dyn Planejamento, saida: &mut dyn Write) -> Result<(), String> {
    let contas = plan.contas().map_err(|e| e.to_string())?;
    let sugestoes = plan.pares_de_conta_sugeridos().map_err(|e| e.to_string())?;
    let do_extrato = plan.contas_do_extrato().map_err(|e| e.to_string())?;
    let rotulador = Rotulador::novo(&contas, &do_extrato);
    let erro = |e: std::io::Error| e.to_string();
    writeln!(saida, "{} código(s) de conta", contas.len()).map_err(erro)?;
    for c in &contas {
        let apelido = c.apelido.as_deref().unwrap_or("(sem apelido)");
        match &c.conta_do_extrato {
            Some(conta) => writeln!(
                saida,
                "{} | conta do extrato: {conta} | apelido: {apelido} | rótulo: {}",
                c.codigo,
                rotulador.rotular(conta)
            ),
            None => writeln!(
                saida,
                "{} | sem par | apelido: {apelido} | rótulo: {}",
                c.codigo, c.codigo
            ),
        }
        .map_err(erro)?;
        if c.conta_do_extrato.is_none()
            && let Some(s) = sugestoes.iter().find(|s| s.codigo == c.codigo)
        {
            writeln!(
                saida,
                "    sugestão: {} ({} evidência(s): {})",
                s.conta_do_extrato, s.evidencias, s.motivo
            )
            .map_err(erro)?;
        }
    }
    if !do_extrato.is_empty() {
        writeln!(saida, "contas do extrato e o rótulo de cada uma:").map_err(erro)?;
        for conta in &do_extrato {
            writeln!(saida, "    {conta} -> {}", rotulador.rotular(conta)).map_err(erro)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn le_cada_comando() {
        let p = ler_pedido(&v(&["--inventario", "/cofre", "listar"])).expect("listar");
        assert_eq!(p.comando, Comando::Listar);
        assert_eq!(p.autor, Autor::Usuario);

        let p = ler_pedido(&v(&[
            "--inventario",
            "/cofre",
            "ligar",
            "CX-9",
            "BANCO EXEMPLO · 000000-1",
            "--autor",
            "ia:modelo-x",
        ]))
        .expect("ligar");
        assert_eq!(
            p.comando,
            Comando::Ligar {
                codigo: "CX-9".into(),
                conta: "BANCO EXEMPLO · 000000-1".into()
            }
        );
        assert_eq!(
            p.autor,
            Autor::Ia {
                modelo: "modelo-x".into()
            }
        );

        let p = ler_pedido(&v(&["--inventario", "/c", "apelido", "CX-9", "--limpar"])).expect("a");
        assert_eq!(
            p.comando,
            Comando::Apelido {
                codigo: "CX-9".into(),
                texto: None
            }
        );
    }

    #[test]
    fn recusa_o_que_nao_entende() {
        assert!(ler_pedido(&v(&["listar"])).is_err(), "sem cofre");
        assert!(ler_pedido(&v(&["--inventario", "/c", "ligar", "CX-9"])).is_err());
        assert!(ler_pedido(&v(&["--inventario", "/c", "apelido", "CX-9"])).is_err());
        assert!(
            ler_pedido(&v(&[
                "--inventario",
                "/c",
                "apelido",
                "CX-9",
                "x",
                "--limpar"
            ]))
            .is_err()
        );
        assert!(ler_pedido(&v(&["--inventario", "/c", "listar", "--autor", "fulano"])).is_err());
        assert!(ler_pedido(&v(&["--inventario", "/c", "listar", "--zzz"])).is_err());
    }

    use sbf_planejamento::{
        Banco, Centavos, Data, FonteDeMovimentos, MovimentoDoExtrato, PlanejamentoSqlite, SemSaldos,
    };

    /// O relógio fixado em 2026-11-16 12:00 UTC.
    const RELOGIO: i64 = 1_794_830_400;

    struct Fixa;

    impl FonteDeMovimentos for Fixa {
        fn movimentos(&self, de: Data, ate: Data) -> Vec<MovimentoDoExtrato> {
            let dia = Data::nova(2026, 11, 10).expect("data");
            [
                "BANCO EXEMPLO · 000000-1",
                "BANCO EXEMPLO Poupança · 000000-1",
            ]
            .iter()
            .enumerate()
            .map(|(i, conta)| MovimentoDoExtrato {
                movimento: format!("pluggy:txn_{i}"),
                data: dia,
                valor: Centavos(100),
                conta_do_extrato: (*conta).to_owned(),
                descricao: "X".to_owned(),
                tipo: "credito".to_owned(),
                contraparte: None,
                contraparte_doc: None,
            })
            .filter(|m| de <= m.data && m.data <= ate)
            .collect()
        }
    }

    fn abrir_de_teste(cofre: &Path) -> Result<Box<dyn Planejamento>, String> {
        let banco = Banco::abrir(cofre.join("planejamento.sqlite")).map_err(|e| e.to_string())?;
        Ok(Box::new(
            PlanejamentoSqlite::novo(banco, Box::new(SemSaldos))
                .com_movimentos(Box::new(Fixa))
                .com_relogio(|| RELOGIO),
        ))
    }

    fn rodar_de_teste(cofre: &Path, resto: &[&str]) -> Result<String, String> {
        let mut args = v(&["--inventario", cofre.to_str().expect("utf8")]);
        args.extend(resto.iter().map(|s| (*s).to_owned()));
        let pedido = ler_pedido(&args)?;
        let mut saida = Vec::new();
        executar(&pedido, &abrir_de_teste, &mut saida)?;
        Ok(String::from_utf8(saida).expect("utf8"))
    }

    #[test]
    fn ligar_apelidar_listar_e_desfazer_passam_pela_mesma_alteracao() {
        let cofre = tempfile::tempdir().expect("cofre");
        let c = cofre.path();
        // Sem nenhum código ainda: ligar cria o par mesmo assim.
        let r =
            rodar_de_teste(c, &["ligar", "CX-0001", "BANCO EXEMPLO · 000000-1"]).expect("ligar");
        assert!(r.starts_with("ok: CX-0001"));
        rodar_de_teste(c, &["apelido", "CX-0001", "Reserva", "--autor", "mcp"]).expect("apelido");

        let lista = rodar_de_teste(c, &["listar"]).expect("listar");
        assert!(lista.contains("CX-0001 | conta do extrato: BANCO EXEMPLO · 000000-1"));
        assert!(lista.contains("apelido: Reserva | rótulo: CX-0001 · Reserva"));
        // A poupança de mesmo número aparece, com o rótulo derivado da corrente.
        assert!(lista.contains("BANCO EXEMPLO Poupança · 000000-1 -> Poupança CX-0001 · Reserva"));

        // Tudo no histórico, com o autor, e desfazível.
        let mut plan = abrir_de_teste(c).expect("abrir");
        let h = plan.historico(Some("CX-0001"), 10).expect("historico");
        assert_eq!(h.len(), 2);
        assert!(h.iter().any(|e| e.autor == Autor::Mcp));
        let apelido = h.iter().find(|e| e.autor == Autor::Mcp).expect("apelido");
        plan.desfazer(&apelido.id, Autor::Usuario)
            .expect("desfazer");
        let lista = rodar_de_teste(c, &["listar"]).expect("listar");
        assert!(lista.contains("apelido: (sem apelido) | rótulo: CX-0001"));

        rodar_de_teste(c, &["desligar", "CX-0001"]).expect("desligar");
        let lista = rodar_de_teste(c, &["listar"]).expect("listar");
        assert!(!lista.contains("conta do extrato: BANCO EXEMPLO"));
    }

    #[test]
    fn limpar_tira_o_apelido_e_codigo_vazio_e_recusado() {
        let cofre = tempfile::tempdir().expect("cofre");
        let c = cofre.path();
        rodar_de_teste(c, &["apelido", "CX-7", "Um"]).expect("dar");
        rodar_de_teste(c, &["apelido", "CX-7", "--limpar"]).expect("tirar");
        let lista = rodar_de_teste(c, &["listar"]).expect("listar");
        assert!(lista.starts_with("0 código(s)"), "{lista}");
        assert!(rodar_de_teste(c, &["apelido", "  ", "Um"]).is_err());
    }
}
