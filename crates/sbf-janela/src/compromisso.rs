//! O formulário "Adicionar compromisso" do Controle mensal, sem egui (testável): os campos
//! como texto digitado, a validação (valores em centavos) e a `Alteracao::CriarRecorrente` que
//! sai dele.

use sbf_planejamento::{
    Alteracao, Centavos, Certeza, Competencia, Data, Fonte, Frequencia, Grupo, Natureza,
    NovoRecorrente, Prazo, Sentido, Versao,
};

use crate::controle::competencia_de_texto;

/// As naturezas que o formulário oferece (a Passagem se divide pelo sentido).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaturezaEscolhida {
    Entrada,
    Saida,
    DescontoEmFolha,
    /// Compra no cartão: `Natureza::DentroDaFatura`.
    CompraNoCartao,
    PassagemQueEntra,
    PassagemQueSai,
}

impl NaturezaEscolhida {
    pub const TODAS: [NaturezaEscolhida; 6] = [
        NaturezaEscolhida::Entrada,
        NaturezaEscolhida::Saida,
        NaturezaEscolhida::DescontoEmFolha,
        NaturezaEscolhida::CompraNoCartao,
        NaturezaEscolhida::PassagemQueEntra,
        NaturezaEscolhida::PassagemQueSai,
    ];

    pub fn rotulo(self) -> &'static str {
        match self {
            NaturezaEscolhida::Entrada => "Entrada",
            NaturezaEscolhida::Saida => "Saída",
            NaturezaEscolhida::DescontoEmFolha => "Desconto em folha",
            NaturezaEscolhida::CompraNoCartao => "Compra no cartão",
            NaturezaEscolhida::PassagemQueEntra => "Passagem (entra)",
            NaturezaEscolhida::PassagemQueSai => "Passagem (sai)",
        }
    }

    pub fn dica(self) -> &'static str {
        match self {
            NaturezaEscolhida::Entrada => "Dinheiro que entra na conta.",
            NaturezaEscolhida::Saida => "Sai da conta ou do cartão: despesa, assinatura, parcela.",
            NaturezaEscolhida::DescontoEmFolha => {
                "Retido no contracheque: aparece, mas não sai do caixa de novo."
            }
            NaturezaEscolhida::CompraNoCartao => {
                "O caixa sai pela fatura, que já está no plano: não soma duas vezes."
            }
            NaturezaEscolhida::PassagemQueEntra | NaturezaEscolhida::PassagemQueSai => {
                "Dinheiro de passagem (bolão): fica fora de receita e de despesa."
            }
        }
    }

    pub fn natureza(self) -> Natureza {
        match self {
            NaturezaEscolhida::Entrada => Natureza::Entrada,
            NaturezaEscolhida::Saida => Natureza::Saida,
            NaturezaEscolhida::DescontoEmFolha => Natureza::DescontoEmFolha,
            NaturezaEscolhida::CompraNoCartao => Natureza::DentroDaFatura,
            NaturezaEscolhida::PassagemQueEntra => Natureza::Passagem {
                sentido: Sentido::Entrada,
            },
            NaturezaEscolhida::PassagemQueSai => Natureza::Passagem {
                sentido: Sentido::Saida,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequenciaEscolhida {
    Mensal,
    CadaNMeses,
    Anual,
    /// Evento único: um plano avulso (um presente).
    Unica,
}

impl FrequenciaEscolhida {
    pub const TODAS: [FrequenciaEscolhida; 4] = [
        FrequenciaEscolhida::Mensal,
        FrequenciaEscolhida::CadaNMeses,
        FrequenciaEscolhida::Anual,
        FrequenciaEscolhida::Unica,
    ];

    pub fn rotulo(self) -> &'static str {
        match self {
            FrequenciaEscolhida::Mensal => "Todo mês",
            FrequenciaEscolhida::CadaNMeses => "A cada N meses",
            FrequenciaEscolhida::Anual => "Uma vez por ano",
            FrequenciaEscolhida::Unica => "Só uma vez (plano avulso)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrazoEscolhido {
    SemFim,
    AteMes,
    Parcelas,
}

impl PrazoEscolhido {
    pub const TODOS: [PrazoEscolhido; 3] = [
        PrazoEscolhido::SemFim,
        PrazoEscolhido::AteMes,
        PrazoEscolhido::Parcelas,
    ];

    pub fn rotulo(self) -> &'static str {
        match self {
            PrazoEscolhido::SemFim => "Sem fim",
            PrazoEscolhido::AteMes => "Até um mês",
            PrazoEscolhido::Parcelas => "N parcelas",
        }
    }
}

/// O formulário, com tudo como texto digitado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormularioDeCompromisso {
    pub nome: String,
    /// O grupo escolhido da lista.
    pub grupo: String,
    /// Um grupo novo; quando preenchido, vale no lugar do escolhido.
    pub grupo_novo: String,
    pub subgrupo: String,
    pub natureza: NaturezaEscolhida,
    pub valor: String,
    pub sem_valor: bool,
    pub frequencia: FrequenciaEscolhida,
    /// O dia do mês (mensal, a cada N meses e anual).
    pub dia: String,
    /// A cada N meses.
    pub n_meses: String,
    /// O mês do ano (anual), de 1 a 12.
    pub mes_do_ano: String,
    /// A data do evento único (`dd/mm/aaaa`).
    pub data_unica: String,
    /// O mês em que o compromisso começa (`mm/aaaa`).
    pub desde: String,
    pub prazo: PrazoEscolhido,
    /// O mês final (`mm/aaaa`; ele não entra).
    pub ate_mes: String,
    pub parcelas: String,
    pub conta: String,
    pub centro: String,
    pub estimado: bool,
    /// A posição (prioridade) do compromisso novo: depois do último que existe.
    pub posicao: i64,
}

impl FormularioDeCompromisso {
    /// Um formulário em branco: todo mês, sem fim, a partir do mês de `hoje`.
    pub fn novo(hoje: Data, grupos: &[Grupo], posicao: i64) -> Self {
        let h = Competencia::de_data(hoje);
        Self {
            nome: String::new(),
            grupo: grupos.first().map(|g| g.nome.clone()).unwrap_or_default(),
            grupo_novo: String::new(),
            subgrupo: String::new(),
            natureza: NaturezaEscolhida::Saida,
            valor: String::new(),
            sem_valor: false,
            frequencia: FrequenciaEscolhida::Mensal,
            dia: hoje.dia.to_string(),
            n_meses: "3".into(),
            mes_do_ano: h.mes.to_string(),
            data_unica: String::new(),
            desde: crate::controle::competencia_texto(h),
            prazo: PrazoEscolhido::SemFim,
            ate_mes: String::new(),
            parcelas: String::new(),
            conta: String::new(),
            centro: String::new(),
            estimado: false,
            posicao,
        }
    }

    /// O grupo que vale: o novo, se digitado; senão o escolhido.
    pub fn grupo_efetivo(&self) -> String {
        let novo = self.grupo_novo.trim();
        if novo.is_empty() {
            self.grupo.trim().to_owned()
        } else {
            novo.to_owned()
        }
    }

    /// O grupo digitado ainda não existe na lista (precisa ser criado antes).
    pub fn precisa_criar_grupo(&self, grupos: &[Grupo]) -> bool {
        let g = self.grupo_efetivo();
        !g.is_empty() && !grupos.iter().any(|x| x.nome.eq_ignore_ascii_case(&g))
    }

    /// O valor em centavos: `None` com "sem valor"; erro se vazio, inválido ou não positivo.
    pub fn valor_em_centavos(&self) -> Result<Option<Centavos>, String> {
        if self.sem_valor {
            return Ok(None);
        }
        let t = self.valor.trim();
        if t.is_empty() {
            return Err("Digite o valor ou marque \"sem valor\".".into());
        }
        match crate::janela::centavos_de(t) {
            Some(v) if v > 0 => Ok(Some(Centavos(v))),
            Some(_) => Err("O valor tem de ser maior que zero.".into()),
            None => Err("Valor inválido (use 312,44).".into()),
        }
    }

    fn numero(texto: &str, nome: &str, de: u32, ate: u32) -> Result<u32, String> {
        let n: u32 = texto
            .trim()
            .parse()
            .map_err(|_| format!("{nome}: digite um número."))?;
        if (de..=ate).contains(&n) {
            Ok(n)
        } else {
            Err(format!("{nome}: de {de} a {ate}."))
        }
    }

    fn mes_de_inicio(&self) -> Result<Competencia, String> {
        competencia_de_texto(&self.desde)
            .ok_or_else(|| "Mês inicial inválido (use 12/2026).".into())
    }

    /// A frequência e o mês em que o compromisso começa, já validados.
    fn frequencia_e_inicio(&self) -> Result<(Frequencia, Competencia), String> {
        match self.frequencia {
            FrequenciaEscolhida::Mensal => {
                let dia = Self::numero(&self.dia, "Dia do mês", 1, 31)?;
                Ok((Frequencia::Mensal { dia: dia as u8 }, self.mes_de_inicio()?))
            }
            FrequenciaEscolhida::CadaNMeses => {
                let n = Self::numero(&self.n_meses, "N de meses", 2, 60)?;
                let dia = Self::numero(&self.dia, "Dia do mês", 1, 31)?;
                let primeira = self.mes_de_inicio()?;
                Ok((
                    Frequencia::CadaNMeses {
                        n: n as u8,
                        dia: dia as u8,
                        primeira,
                    },
                    primeira,
                ))
            }
            FrequenciaEscolhida::Anual => {
                let mes = Self::numero(&self.mes_do_ano, "Mês do ano", 1, 12)?;
                let dia = Self::numero(&self.dia, "Dia do mês", 1, 31)?;
                Ok((
                    Frequencia::Anual {
                        mes: mes as u8,
                        dia: dia as u8,
                    },
                    self.mes_de_inicio()?,
                ))
            }
            FrequenciaEscolhida::Unica => {
                let iso = crate::janela::data_iso(self.data_unica.trim())
                    .ok_or("Data inválida (use 15/02/2027).")?;
                let data = Data::de_iso(&iso).ok_or("Data inválida (use 15/02/2027).")?;
                Ok((Frequencia::Unica { data }, Competencia::de_data(data)))
            }
        }
    }

    fn prazo_validado(&self, inicio: Competencia) -> Result<Prazo, String> {
        if self.frequencia == FrequenciaEscolhida::Unica {
            return Ok(Prazo::SemFim);
        }
        match self.prazo {
            PrazoEscolhido::SemFim => Ok(Prazo::SemFim),
            PrazoEscolhido::AteMes => {
                let ate = competencia_de_texto(&self.ate_mes)
                    .ok_or("Mês final inválido (use 12/2027).")?;
                if ate <= inicio {
                    return Err("O mês final vem depois do inicial (o final não entra).".into());
                }
                Ok(Prazo::AteCompetencia(ate))
            }
            PrazoEscolhido::Parcelas => {
                let total = Self::numero(&self.parcelas, "Parcelas", 1, 360)?;
                Ok(Prazo::Parcelas {
                    total,
                    primeira: inicio,
                })
            }
        }
    }

    /// A alteração que cria o compromisso, ou o que está errado, em português.
    pub fn alteracao(&self) -> Result<Alteracao, String> {
        let nome = self.nome.trim();
        if nome.is_empty() {
            return Err("Dê um nome ao compromisso.".into());
        }
        let grupo = self.grupo_efetivo();
        if grupo.is_empty() {
            return Err("Escolha um grupo ou digite um novo.".into());
        }
        let valor = self.valor_em_centavos()?;
        let (frequencia, inicio) = self.frequencia_e_inicio()?;
        let prazo = self.prazo_validado(inicio)?;
        let texto = |t: &str| {
            let t = t.trim();
            (!t.is_empty()).then(|| t.to_owned())
        };
        Ok(Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: nome.to_owned(),
                grupo,
                subgrupo: texto(&self.subgrupo),
                posicao: self.posicao,
                natureza: self.natureza.natureza(),
                prazo,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: inicio,
                    valor,
                    certeza: if self.estimado || valor.is_none() {
                        Certeza::Estimado
                    } else {
                        Certeza::Confirmado
                    },
                    fonte: Fonte::Usuario,
                    frequencia,
                    conta: texto(&self.conta),
                    centro: texto(&self.centro),
                    fornecedor: None,
                    categoria: None,
                    nota: None,
                },
            },
            origem: None,
        })
    }

    /// A criação do grupo, quando o digitado ainda não existe. Vai antes da alteração
    /// principal.
    pub fn alteracao_do_grupo(&self, grupos: &[Grupo]) -> Option<Alteracao> {
        self.precisa_criar_grupo(grupos)
            .then(|| Alteracao::CriarGrupo {
                nome: self.grupo_efetivo(),
                posicao: grupos.iter().map(|g| g.posicao).max().unwrap_or(0) + 10,
            })
    }

    /// As alterações em ordem: o grupo novo (se houver) e o compromisso.
    pub fn alteracoes(&self, grupos: &[Grupo]) -> Result<Vec<Alteracao>, String> {
        let principal = self.alteracao()?;
        Ok(self
            .alteracao_do_grupo(grupos)
            .into_iter()
            .chain(std::iter::once(principal))
            .collect())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hoje() -> Data {
        Data::de_iso("2026-11-16").unwrap()
    }

    fn grupos() -> Vec<Grupo> {
        vec![
            Grupo {
                nome: "Lar".into(),
                posicao: 0,
            },
            Grupo {
                nome: "Lazer".into(),
                posicao: 10,
            },
        ]
    }

    fn pronto() -> FormularioDeCompromisso {
        let mut f = FormularioDeCompromisso::novo(hoje(), &grupos(), 500);
        f.nome = "Academia".into();
        f.valor = "129,90".into();
        f.dia = "10".into();
        f
    }

    fn criar(a: Alteracao) -> NovoRecorrente {
        let Alteracao::CriarRecorrente { recorrente, .. } = a else {
            panic!("devia criar");
        };
        recorrente
    }

    #[test]
    fn mensal_sem_fim_em_centavos() {
        let n = criar(pronto().alteracao().unwrap());
        assert_eq!(n.nome, "Academia");
        assert_eq!(n.grupo, "Lar");
        assert_eq!(n.prazo, Prazo::SemFim);
        assert_eq!(n.posicao, 500);
        let v = n.primeira_versao;
        assert_eq!(v.valor, Some(Centavos(12_990)));
        assert_eq!(v.frequencia, Frequencia::Mensal { dia: 10 });
        assert_eq!(v.desde, Competencia { ano: 2026, mes: 11 });
        assert_eq!(v.certeza, Certeza::Confirmado);
        assert_eq!(v.fonte, Fonte::Usuario);
    }

    #[test]
    fn valor_aceita_milhar_e_recusa_lixo_zero_e_centavo_a_mais() {
        let mut f = pronto();
        f.valor = "1.234,56".into();
        assert_eq!(f.valor_em_centavos(), Ok(Some(Centavos(123_456))));
        f.valor = "312".into();
        assert_eq!(f.valor_em_centavos(), Ok(Some(Centavos(31_200))));
        f.valor = "0,5".into();
        assert_eq!(f.valor_em_centavos(), Ok(Some(Centavos(50))));
        for ruim in ["", "abc", "1,234", "0", "-5,00"] {
            f.valor = ruim.into();
            assert!(f.valor_em_centavos().is_err(), "{ruim:?}");
        }
        f.sem_valor = true;
        assert_eq!(f.valor_em_centavos(), Ok(None));
    }

    #[test]
    fn sem_valor_vira_estimado_e_sem_valor() {
        let mut f = pronto();
        f.sem_valor = true;
        f.valor.clear();
        let v = criar(f.alteracao().unwrap()).primeira_versao;
        assert_eq!(v.valor, None);
        assert_eq!(v.certeza, Certeza::Estimado);
    }

    #[test]
    fn nome_e_grupo_sao_obrigatorios() {
        let mut f = pronto();
        f.nome = "  ".into();
        assert!(f.alteracao().unwrap_err().contains("nome"));
        let mut f = pronto();
        f.grupo.clear();
        assert!(f.alteracao().unwrap_err().contains("grupo"));
        f.grupo_novo = "Pets".into();
        assert!(f.alteracao().is_ok());
    }

    #[test]
    fn grupo_novo_vem_antes_do_compromisso() {
        let mut f = pronto();
        f.grupo_novo = "Pets".into();
        let v = f.alteracoes(&grupos()).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(
            v[0],
            Alteracao::CriarGrupo {
                nome: "Pets".into(),
                posicao: 20
            }
        );
        assert_eq!(criar(v[1].clone()).grupo, "Pets");
        // Um grupo que já existe (mesmo com outra caixa) não se cria de novo.
        f.grupo_novo = "lazer".into();
        assert_eq!(f.alteracoes(&grupos()).unwrap().len(), 1);
    }

    #[test]
    fn dia_fora_do_mes_e_recusado() {
        let mut f = pronto();
        for ruim in ["0", "32", "x", ""] {
            f.dia = ruim.into();
            assert!(f.alteracao().is_err(), "{ruim:?}");
        }
        f.dia = "31".into();
        assert!(f.alteracao().is_ok());
    }

    #[test]
    fn a_cada_n_meses_comeca_no_mes_escolhido() {
        let mut f = pronto();
        f.frequencia = FrequenciaEscolhida::CadaNMeses;
        f.n_meses = "3".into();
        f.desde = "01/2027".into();
        let v = criar(f.alteracao().unwrap()).primeira_versao;
        assert_eq!(
            v.frequencia,
            Frequencia::CadaNMeses {
                n: 3,
                dia: 10,
                primeira: Competencia { ano: 2027, mes: 1 }
            }
        );
        assert_eq!(v.desde, Competencia { ano: 2027, mes: 1 });
        f.n_meses = "1".into();
        assert!(f.alteracao().is_err());
    }

    #[test]
    fn anual_pede_mes_e_dia() {
        let mut f = pronto();
        f.frequencia = FrequenciaEscolhida::Anual;
        f.mes_do_ano = "12".into();
        f.dia = "25".into();
        let v = criar(f.alteracao().unwrap()).primeira_versao;
        assert_eq!(v.frequencia, Frequencia::Anual { mes: 12, dia: 25 });
        f.mes_do_ano = "13".into();
        assert!(f.alteracao().is_err());
    }

    #[test]
    fn plano_avulso_e_unico_e_sem_prazo() {
        let mut f = pronto();
        f.nome = "Presente da Ana".into();
        f.frequencia = FrequenciaEscolhida::Unica;
        f.data_unica = "15/02/2027".into();
        // O prazo escolhido não vale para um evento único.
        f.prazo = PrazoEscolhido::Parcelas;
        f.parcelas = "".into();
        let n = criar(f.alteracao().unwrap());
        assert_eq!(n.prazo, Prazo::SemFim);
        assert_eq!(
            n.primeira_versao.frequencia,
            Frequencia::Unica {
                data: Data::de_iso("2027-02-15").unwrap()
            }
        );
        assert_eq!(n.primeira_versao.desde, Competencia { ano: 2027, mes: 2 });
        f.data_unica = "31/13/2027".into();
        assert!(f.alteracao().is_err());
    }

    #[test]
    fn prazo_ate_mes_e_parcelas() {
        let mut f = pronto();
        f.prazo = PrazoEscolhido::AteMes;
        f.ate_mes = "11/2026".into();
        assert!(f.alteracao().unwrap_err().contains("depois"));
        f.ate_mes = "05/2027".into();
        assert_eq!(
            criar(f.alteracao().unwrap()).prazo,
            Prazo::AteCompetencia(Competencia { ano: 2027, mes: 5 })
        );
        f.prazo = PrazoEscolhido::Parcelas;
        f.parcelas = "12".into();
        assert_eq!(
            criar(f.alteracao().unwrap()).prazo,
            Prazo::Parcelas {
                total: 12,
                primeira: Competencia { ano: 2026, mes: 11 }
            }
        );
        f.parcelas = "0".into();
        assert!(f.alteracao().is_err());
    }

    #[test]
    fn naturezas_chegam_certas() {
        let casos = [
            (NaturezaEscolhida::Entrada, Natureza::Entrada),
            (NaturezaEscolhida::Saida, Natureza::Saida),
            (
                NaturezaEscolhida::DescontoEmFolha,
                Natureza::DescontoEmFolha,
            ),
            (NaturezaEscolhida::CompraNoCartao, Natureza::DentroDaFatura),
            (
                NaturezaEscolhida::PassagemQueEntra,
                Natureza::Passagem {
                    sentido: Sentido::Entrada,
                },
            ),
        ];
        for (e, n) in casos {
            let mut f = pronto();
            f.natureza = e;
            assert_eq!(criar(f.alteracao().unwrap()).natureza, n);
        }
    }

    #[test]
    fn conta_e_centro_em_branco_ficam_sem_valor() {
        let mut f = pronto();
        f.conta = " Nubank ".into();
        let v = criar(f.alteracao().unwrap()).primeira_versao;
        assert_eq!(v.conta.as_deref(), Some("Nubank"));
        assert_eq!(v.centro, None);
    }
}
