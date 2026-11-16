//! Saldos: o último saldo conhecido de cada conta, do bruto da Pluggy guardado no cofre
//! (`entrada/pluggy/bruto-AAAA-MM-DD/contas.jsonl`, uma conta por linha). A data da coleta é
//! a do nome da pasta. Cartão de crédito fica de fora: o saldo dele é fatura, não dinheiro
//! disponível.

use std::path::Path;

use sbf_planejamento::{FonteDeSaldos, Saldo};

use crate::texto::centavos_de_f64;
use crate::{ErroPlanilha, Resultado};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaldosDaPluggy {
    saldos: Vec<Saldo>,
}

impl SaldosDaPluggy {
    /// Lê o bruto do cofre. Sem nenhuma coleta, devolve vazio (não é erro).
    pub fn ler(cofre: &Path) -> Resultado<SaldosDaPluggy> {
        let pasta = cofre.join("entrada").join("pluggy");
        let Ok(entradas) = std::fs::read_dir(&pasta) else {
            return Ok(SaldosDaPluggy::default());
        };
        let mut coletas: Vec<(String, std::path::PathBuf)> = entradas
            .filter_map(Result::ok)
            .filter_map(|e| {
                let nome = e.file_name().to_string_lossy().into_owned();
                let data = nome.strip_prefix("bruto-")?.to_owned();
                (data.len() == 10).then(|| (data, e.path()))
            })
            .collect();
        // Da mais recente para a mais antiga: a primeira vez que uma conta aparece vale.
        coletas.sort_by(|a, b| b.0.cmp(&a.0));
        let mut vistos = std::collections::HashSet::new();
        let mut saldos = Vec::new();
        for (data, caminho) in coletas {
            let arquivo = caminho.join("contas.jsonl");
            if !arquivo.exists() {
                continue;
            }
            let texto = std::fs::read_to_string(&arquivo)
                .map_err(|e| ErroPlanilha::Saldos(format!("não deu para ler as contas ({e})")))?;
            for linha in texto.lines().filter(|l| !l.trim().is_empty()) {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(linha) else {
                    continue;
                };
                if v["type"].as_str() == Some("CREDIT") {
                    continue;
                }
                let (Some(id), Some(nome), Some(saldo)) = (
                    v["id"].as_str(),
                    v["name"].as_str(),
                    v["balance"].as_f64().and_then(centavos_de_f64),
                ) else {
                    continue;
                };
                if vistos.insert(id.to_owned()) {
                    saldos.push(Saldo {
                        conta: nome.to_owned(),
                        valor: saldo,
                        quando: format!("{data}T00:00:00Z"),
                    });
                }
            }
        }
        Ok(SaldosDaPluggy { saldos })
    }
}

impl FonteDeSaldos for SaldosDaPluggy {
    fn saldos(&self) -> Vec<Saldo> {
        self.saldos.clone()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::Centavos;

    fn coleta(cofre: &Path, data: &str, contas: &[&str]) {
        let p = cofre.join("entrada/pluggy").join(format!("bruto-{data}"));
        std::fs::create_dir_all(&p).expect("pasta");
        std::fs::write(p.join("contas.jsonl"), contas.join("\n")).expect("contas");
    }

    #[test]
    fn ultimo_saldo_de_cada_conta_com_a_data_da_coleta() {
        let cofre = tempfile::tempdir().expect("cofre");
        coleta(
            cofre.path(),
            "2026-11-01",
            &[
                r#"{"id":"a","name":"Conta A","type":"BANK","balance":100.10}"#,
                r#"{"id":"b","name":"Conta B","type":"BANK","balance":50}"#,
            ],
        );
        coleta(
            cofre.path(),
            "2026-11-15",
            &[
                r#"{"id":"a","name":"Conta A","type":"BANK","balance":200.2}"#,
                r#"{"id":"c","name":"Cartão","type":"CREDIT","balance":900}"#,
                "isto nao e json",
                r#"{"id":"d","name":"Sem saldo","type":"BANK"}"#,
            ],
        );
        let s = SaldosDaPluggy::ler(cofre.path()).expect("ler").saldos();
        assert_eq!(s.len(), 2);
        let a = s.iter().find(|x| x.conta == "Conta A").expect("a");
        assert_eq!(
            (a.valor, a.quando.as_str()),
            (Centavos(20_020), "2026-11-15T00:00:00Z")
        );
        let b = s.iter().find(|x| x.conta == "Conta B").expect("b");
        assert_eq!(
            (b.valor, b.quando.as_str()),
            (Centavos(5_000), "2026-11-01T00:00:00Z")
        );
    }

    #[test]
    fn sem_coleta_e_vazio() {
        let cofre = tempfile::tempdir().expect("cofre");
        assert!(
            SaldosDaPluggy::ler(cofre.path())
                .expect("ler")
                .saldos()
                .is_empty()
        );
    }
}
