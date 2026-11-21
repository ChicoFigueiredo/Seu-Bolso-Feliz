//! O **rótulo da conta**: o que a tela mostra no lugar da conta do extrato.
//!
//! - par confirmado: o código, seguido do apelido quando houver (`CX-0001 · Apelido`);
//! - poupança vinculada: a conta do extrato `"<BANCO> Poupança · <num>"` não tem par próprio,
//!   mas a corrente de mesmo número (`"<BANCO> · <num>"`) tem; vale `Poupança <rótulo da
//!   corrente>`. Se a própria poupança tiver par, vale o dela;
//! - sem par: a conta do extrato como veio.
//!
//! Isto é **só exibição**. A conciliação continua exigindo a conta do extrato exata do par
//! (`conciliacao.rs` não olha para cá). A tela mostra o rótulo e guarda a conta do extrato
//! no tooltip; filtros e buscas aceitam o rótulo e a conta do extrato (o nome do banco).

use std::collections::{BTreeSet, HashMap};

use crate::ContaDoPlanejamento;
use crate::conciliacao::normalizar;

/// O separador entre instituição e número, e entre código e apelido.
const SEPARADOR: &str = " · ";
/// O tipo que a Pluggy põe antes do número numa conta de poupança.
const POUPANCA: &str = "Poupança";

/// Calcula rótulos a partir dos pares confirmados e dos apelidos. Barato de clonar e de
/// construir; quem mostra contas constrói um por leitura do planejamento.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rotulador {
    /// Conta do extrato -> (código, apelido).
    pares: HashMap<String, (String, Option<String>)>,
    /// As contas do extrato que se conhecem (para expandir um filtro), em ordem.
    do_extrato: BTreeSet<String>,
}

impl Rotulador {
    /// `contas` são as do planejamento (código, apelido e par); `do_extrato` as contas que
    /// o extrato traz (podem incluir poupanças sem par).
    #[must_use]
    pub fn novo(contas: &[ContaDoPlanejamento], do_extrato: &[String]) -> Self {
        let mut pares = HashMap::new();
        for c in contas {
            let Some(conta) = c.conta_do_extrato.as_deref().map(str::trim) else {
                continue;
            };
            let apelido = c
                .apelido
                .as_deref()
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_owned);
            pares.insert(conta.to_owned(), (c.codigo.trim().to_owned(), apelido));
        }
        let mut conhecidas: BTreeSet<String> = do_extrato
            .iter()
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect();
        conhecidas.extend(pares.keys().cloned());
        Self {
            pares,
            do_extrato: conhecidas,
        }
    }

    /// O rótulo, se a conta tem par (exato ou, na poupança, o da corrente). `None`: a conta
    /// do extrato aparece como veio.
    #[must_use]
    pub fn rotulo(&self, conta_do_extrato: &str) -> Option<String> {
        let conta = conta_do_extrato.trim();
        if let Some((codigo, apelido)) = self.pares.get(conta) {
            return Some(texto_do_par(codigo, apelido.as_deref()));
        }
        let corrente = corrente_da_poupanca(conta)?;
        let (codigo, apelido) = self.pares.get(&corrente)?;
        Some(format!(
            "{POUPANCA} {}",
            texto_do_par(codigo, apelido.as_deref())
        ))
    }

    /// O rótulo, ou a conta do extrato como veio.
    #[must_use]
    pub fn rotular(&self, conta_do_extrato: &str) -> String {
        self.rotulo(conta_do_extrato)
            .unwrap_or_else(|| conta_do_extrato.to_owned())
    }

    /// `rotular` para um campo opcional: `None` e vazio ficam como estão.
    #[must_use]
    pub fn rotular_opcional(&self, conta_do_extrato: Option<&str>) -> Option<String> {
        conta_do_extrato.map(|c| self.rotular(c))
    }

    /// O texto de um filtro ou busca acha a conta? Casa o rótulo **ou** a conta do extrato,
    /// sem caixa e sem acento ("reserv" acha "CX-0001 · Reserva"; "caixa" acha a conta
    /// pelo nome do banco).
    #[must_use]
    pub fn casa(&self, conta_do_extrato: &str, texto: &str) -> bool {
        let t = normalizar(texto.trim());
        if t.is_empty() {
            return true;
        }
        normalizar(conta_do_extrato).contains(&t)
            || self
                .rotulo(conta_do_extrato)
                .is_some_and(|r| normalizar(&r).contains(&t))
    }

    /// As contas do extrato conhecidas que o texto acha (pelo rótulo ou pela conta).
    #[must_use]
    pub fn contas_que_casam(&self, texto: &str) -> Vec<String> {
        self.do_extrato
            .iter()
            .filter(|c| self.casa(c, texto))
            .cloned()
            .collect()
    }
}

/// `CX-0001` ou `CX-0001 · Apelido`.
fn texto_do_par(codigo: &str, apelido: Option<&str>) -> String {
    match apelido {
        Some(a) => format!("{codigo}{SEPARADOR}{a}"),
        None => codigo.to_owned(),
    }
}

/// De `"<BANCO> Poupança · <num>"` (ou `Poupanca`, sem acento) tira a conta corrente de
/// mesmo número: `"<BANCO> · <num>"`. `None` se a conta não for de poupança.
fn corrente_da_poupanca(conta: &str) -> Option<String> {
    let (instituicao, numero) = conta.split_once(SEPARADOR)?;
    let (instituicao, numero) = (instituicao.trim(), numero.trim());
    if numero.is_empty() {
        return None;
    }
    let tipo = instituicao.rsplit_once(' ').map_or(instituicao, |(_, t)| t);
    if normalizar(tipo) != normalizar(POUPANCA) {
        return None;
    }
    let banco = instituicao[..instituicao.len() - tipo.len()].trim();
    if banco.is_empty() {
        return None;
    }
    Some(format!("{banco}{SEPARADOR}{numero}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conta(codigo: &str, apelido: Option<&str>, par: Option<&str>) -> ContaDoPlanejamento {
        ContaDoPlanejamento {
            codigo: codigo.to_owned(),
            apelido: apelido.map(str::to_owned),
            conta_do_extrato: par.map(str::to_owned),
        }
    }

    fn rotulador() -> Rotulador {
        Rotulador::novo(
            &[
                conta("CX-0001", Some("Reserva"), Some("BANCO EXEMPLO · 000000-1")),
                conta("CX-0002", None, Some("OUTRO BANCO · 123456-7")),
                conta("CX-0003", Some("Sem par"), None),
                conta(
                    "CX-0004",
                    Some("Poupança própria"),
                    Some("BANCO EXEMPLO Poupança · 777777-7"),
                ),
            ],
            &[
                "BANCO EXEMPLO · 000000-1".to_owned(),
                "BANCO EXEMPLO Poupança · 000000-1".to_owned(),
                "BANCO EXEMPLO · 999999-9".to_owned(),
                "BANCO EXEMPLO Poupança · 999999-9".to_owned(),
            ],
        )
    }

    #[test]
    fn par_exato_com_apelido_mostra_codigo_e_apelido() {
        assert_eq!(
            rotulador().rotular("BANCO EXEMPLO · 000000-1"),
            "CX-0001 · Reserva"
        );
    }

    #[test]
    fn par_exato_sem_apelido_mostra_so_o_codigo() {
        assert_eq!(rotulador().rotular("OUTRO BANCO · 123456-7"), "CX-0002");
    }

    #[test]
    fn apelido_vazio_ou_so_espacos_vale_como_sem_apelido() {
        let r = Rotulador::novo(
            &[conta("CX-9", Some("   "), Some("BANCO EXEMPLO · 1-1"))],
            &[],
        );
        assert_eq!(r.rotular("BANCO EXEMPLO · 1-1"), "CX-9");
    }

    #[test]
    fn poupanca_vinculada_leva_o_rotulo_da_corrente_com_poupanca_na_frente() {
        assert_eq!(
            rotulador().rotular("BANCO EXEMPLO Poupança · 000000-1"),
            "Poupança CX-0001 · Reserva"
        );
        // Sem acento também (o extrato nem sempre acentua).
        assert_eq!(
            rotulador().rotular("BANCO EXEMPLO Poupanca · 000000-1"),
            "Poupança CX-0001 · Reserva"
        );
    }

    #[test]
    fn poupanca_com_par_proprio_vale_o_dela() {
        assert_eq!(
            rotulador().rotular("BANCO EXEMPLO Poupança · 777777-7"),
            "CX-0004 · Poupança própria"
        );
    }

    #[test]
    fn poupanca_cuja_corrente_nao_tem_par_aparece_como_veio() {
        assert_eq!(
            rotulador().rotular("BANCO EXEMPLO Poupança · 999999-9"),
            "BANCO EXEMPLO Poupança · 999999-9"
        );
        assert_eq!(
            rotulador().rotulo("BANCO EXEMPLO Poupança · 999999-9"),
            None
        );
    }

    #[test]
    fn so_o_numero_e_o_banco_iguais_vinculam_a_poupanca() {
        // Outro banco com o mesmo número não é a poupança da corrente.
        assert_eq!(
            rotulador().rotular("OUTRO BANCO Poupança · 000000-1"),
            "OUTRO BANCO Poupança · 000000-1"
        );
    }

    #[test]
    fn sem_par_vira_a_conta_do_extrato_como_veio() {
        assert_eq!(
            rotulador().rotular("QUALQUER · 555555-5"),
            "QUALQUER · 555555-5"
        );
    }

    #[test]
    fn codigo_sem_par_nao_aponta_para_conta_nenhuma() {
        // CX-0003 tem apelido mas nenhum par: nada a rotular.
        assert_eq!(rotulador().rotulo("CX-0003"), None);
    }

    #[test]
    fn vazio_fica_como_esta() {
        let r = rotulador();
        assert_eq!(r.rotular(""), "");
        assert_eq!(r.rotular_opcional(None), None);
        assert_eq!(r.rotular_opcional(Some("")), Some(String::new()));
    }

    #[test]
    fn espacos_em_volta_da_conta_nao_atrapalham() {
        assert_eq!(
            rotulador().rotular("  BANCO EXEMPLO · 000000-1 "),
            "CX-0001 · Reserva"
        );
    }

    #[test]
    fn filtro_aceita_rotulo_codigo_apelido_e_nome_do_banco() {
        let r = rotulador();
        let conta = "BANCO EXEMPLO · 000000-1";
        assert!(r.casa(conta, "reserva"));
        assert!(r.casa(conta, "CX-0001"));
        assert!(r.casa(conta, "cx-0001 · res"));
        assert!(r.casa(conta, "banco exemplo"));
        assert!(r.casa(conta, "000000"));
        assert!(!r.casa(conta, "outro"));
        assert!(r.casa(conta, "  "));
    }

    #[test]
    fn filtro_ignora_acento_e_caixa() {
        let r = rotulador();
        assert!(r.casa("BANCO EXEMPLO Poupança · 000000-1", "POUPANCA cx-0001"));
        assert!(r.casa("BANCO EXEMPLO Poupança · 000000-1", "poupança"));
    }

    #[test]
    fn contas_que_casam_lista_todas_as_conhecidas_do_filtro() {
        let r = rotulador();
        let mut achadas = r.contas_que_casam("reserva");
        achadas.sort();
        let mut esperadas = vec![
            "BANCO EXEMPLO · 000000-1".to_owned(),
            "BANCO EXEMPLO Poupança · 000000-1".to_owned(),
        ];
        esperadas.sort();
        assert_eq!(achadas, esperadas);
        // O nome do banco acha as quatro do banco, vinculadas ou não.
        assert_eq!(r.contas_que_casam("banco exemplo").len(), 5);
    }

    #[test]
    fn nenhuma_conta_sai_da_lista_por_falta_de_par() {
        let r = rotulador();
        assert_eq!(r.contas_que_casam("").len(), 6);
    }
}
