//! O extrato fictício e a conciliação do planejamento fictício: contas do planejamento,
//! pares apelido ↔ conta do extrato, assinaturas e candidatos. Tudo inventado; nada vem do
//! inventário nem da Pluggy reais.

use sbf_planejamento::{
    Alteracao, Assinatura, Autor, Centavos, Competencia, Conciliacao, ContaDoPlanejamento, Data,
    ErroDoPlanejamento, Filtro, IdOcorrencia, MovimentoCandidato, MovimentoDoExtrato, Natureza,
    Ocorrencia, ParDeConta, Periodo, Planejamento, Recibo, Resultado,
};

use super::PlanejamentoFicticio;

/// A conta do extrato onde o salário fictício cai.
pub const CONTA_DO_SALARIO: &str = "BANCO EXEMPLO · 000000-1";
/// Uma segunda conta do extrato, sem par no planejamento.
pub const CONTA_SEM_PAR: &str = "BANCO EXEMPLO · 000000-2";
/// A conta do extrato do Nubank fictício, com par já confirmado.
pub const CONTA_DO_NUBANK: &str = "CARTAO EXEMPLO · 000000-9";
/// O texto da assinatura do salário fictício.
pub const TEXTO_DO_SALARIO: &str = "SALARIO EMPRESA EXEMPLO";
/// O líquido do contracheque fictício (o crédito do mês corrente).
pub const LIQUIDO_DO_SALARIO: Centavos = Centavos(822_600);

/// Maiúsculas e sem acento, para comparar descrições do banco com o texto digitado.
pub fn normalizar_texto(t: &str) -> String {
    t.chars()
        .map(|c| match c {
            'á' | 'à' | 'ã' | 'â' | 'Á' | 'À' | 'Ã' | 'Â' => 'A',
            'é' | 'ê' | 'É' | 'Ê' => 'E',
            'í' | 'Í' => 'I',
            'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
            'ú' | 'Ú' => 'U',
            'ç' | 'Ç' => 'C',
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

fn movimento(
    id: &str,
    data: Data,
    valor: i64,
    conta: &str,
    descricao: &str,
    tipo: &str,
) -> MovimentoDoExtrato {
    MovimentoDoExtrato {
        movimento: format!("pluggy:txn_ficticia_{id}"),
        data,
        valor: Centavos(valor),
        conta_do_extrato: conta.into(),
        descricao: descricao.into(),
        tipo: tipo.into(),
    }
}

impl PlanejamentoFicticio {
    /// O extrato, a assinatura do salário, o par do Nubank e a sugestão do salário do mês.
    pub(super) fn semear_extrato(&mut self, hoje: Data) {
        let h = Competencia::de_data(hoje);
        self.assinaturas.push((
            "rec_salario".into(),
            Assinatura {
                texto: TEXTO_DO_SALARIO.into(),
                janela_dias: 5,
            },
        ));
        self.mapa_contas
            .push(("Nubank".into(), CONTA_DO_NUBANK.into()));
        for (d, valor) in [(-2, 815_400), (-1, 819_900), (0, LIQUIDO_DO_SALARIO.0)] {
            let comp = h.somar_meses(d);
            let Some(oc) = self.ocorrencia_de("rec_salario", comp, hoje) else {
                continue;
            };
            let data = oc.vencimento.somar_dias(-1);
            self.extrato.push(movimento(
                &format!("sal_{:04}{:02}", comp.ano, comp.mes),
                data,
                valor,
                CONTA_DO_SALARIO,
                &format!("{TEXTO_DO_SALARIO} {:02}/{:04}", comp.mes, comp.ano),
                "credito",
            ));
            // Os dois meses de trás já foram conciliados; o do mês corrente fica sugerido.
            self.excecoes.retain(|e| e.ocorrencia != oc.id);
            self.conciliacoes.retain(|c| c.ocorrencia != oc.id);
            self.conciliacoes.push(Conciliacao {
                ocorrencia: oc.id.clone(),
                movimento: format!("pluggy:txn_ficticia_sal_{:04}{:02}", comp.ano, comp.mes),
                valor: Centavos(valor),
                data,
                confirmada: d < 0,
            });
        }
        let base = hoje.somar_dias(-6);
        self.extrato.push(movimento(
            "pix_001",
            base,
            30_000,
            CONTA_DO_SALARIO,
            "PIX RECEBIDO AMIGA EXEMPLO",
            "credito",
        ));
        self.extrato.push(movimento(
            "casa_001",
            base.somar_dias(-1),
            150_000,
            CONTA_SEM_PAR,
            "TRANSFERENCIA RECEBIDA CASA EXEMPLO",
            "credito",
        ));
        self.extrato.push(movimento(
            "mercado_001",
            hoje.somar_dias(-3),
            -23_450,
            CONTA_DO_SALARIO,
            "COMPRA MERCADO EXEMPLO",
            "debito",
        ));
    }

    fn apelido_da(&self, recorrente: &str, comp: Competencia) -> Option<String> {
        let r = self.recorrente_interno(recorrente)?;
        Self::versao_vigente(r, comp)?.conta.clone()
    }

    fn conta_do_apelido(&self, apelido: &str) -> Option<&str> {
        self.mapa_contas
            .iter()
            .find(|(a, _)| a == apelido)
            .map(|(_, c)| c.as_str())
    }

    fn assinatura_de(&self, recorrente: &str) -> Option<&Assinatura> {
        self.assinaturas
            .iter()
            .find(|(r, _)| r == recorrente)
            .map(|(_, a)| a)
    }

    pub(super) fn contas_impl(&self) -> Vec<ContaDoPlanejamento> {
        let mut v: Vec<ContaDoPlanejamento> = Vec::new();
        for r in &self.recorrentes {
            for ver in &r.versoes {
                let Some(a) = &ver.conta else { continue };
                if v.iter().all(|c| &c.apelido != a) {
                    v.push(ContaDoPlanejamento {
                        apelido: a.clone(),
                        conta_do_extrato: self.conta_do_apelido(a).map(str::to_owned),
                    });
                }
            }
        }
        v.sort_by(|a, b| a.apelido.cmp(&b.apelido));
        v
    }

    pub(super) fn pares_sugeridos_impl(&self) -> Vec<ParDeConta> {
        // (apelido, conta) -> (conciliações, créditos que batem com a assinatura)
        let mut pontos: Vec<(String, String, u32, u32)> = Vec::new();
        let mut somar = |apelido: &str, conta: &str, conc: u32, assin: u32| {
            if let Some(p) = pontos
                .iter_mut()
                .find(|(a, c, _, _)| a == apelido && c == conta)
            {
                p.2 += conc;
                p.3 += assin;
            } else {
                pontos.push((apelido.into(), conta.into(), conc, assin));
            }
        };
        for c in &self.conciliacoes {
            let Some(m) = self.extrato.iter().find(|m| m.movimento == c.movimento) else {
                continue;
            };
            if let Some(a) = self.apelido_da(&c.ocorrencia.recorrente, c.ocorrencia.competencia) {
                somar(&a, &m.conta_do_extrato, 1, 0);
            }
        }
        for (rec, ass) in &self.assinaturas {
            let Some(a) = self.apelido_da(rec, Competencia::de_data(self.hoje)) else {
                continue;
            };
            let texto = normalizar_texto(&ass.texto);
            for m in &self.extrato {
                if normalizar_texto(&m.descricao).contains(&texto) {
                    somar(&a, &m.conta_do_extrato, 0, 1);
                }
            }
        }
        let mut saida: Vec<ParDeConta> = Vec::new();
        for (apelido, conta, conc, assin) in pontos {
            if self.conta_do_apelido(&apelido).is_some() {
                continue;
            }
            let evidencias = conc + assin;
            if saida
                .iter()
                .any(|p| p.apelido == apelido && p.evidencias >= evidencias)
            {
                continue;
            }
            let mut partes = Vec::new();
            if conc > 0 {
                partes.push(format!(
                    "{conc} {}",
                    if conc == 1 {
                        "conciliação"
                    } else {
                        "conciliações"
                    }
                ));
            }
            if assin > 0 {
                partes.push(format!(
                    "{assin} {} que bate com a assinatura",
                    if assin == 1 { "crédito" } else { "créditos" }
                ));
            }
            saida.retain(|p| p.apelido != apelido);
            saida.push(ParDeConta {
                apelido,
                conta_do_extrato: conta,
                evidencias,
                motivo: partes.join(" e "),
            });
        }
        saida.sort_by(|a, b| a.apelido.cmp(&b.apelido));
        saida
    }

    pub(super) fn contas_do_extrato_impl(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .extrato
            .iter()
            .map(|m| m.conta_do_extrato.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    pub(super) fn assinatura_impl(&self, recorrente: &str) -> Option<Assinatura> {
        self.assinatura_de(recorrente).cloned()
    }

    fn candidatos_de(&self, o: &Ocorrencia) -> Vec<MovimentoCandidato> {
        let ass = self.assinatura_de(&o.id.recorrente);
        // A lista de escolha manual olha 10 dias; a busca automática usa a janela da assinatura.
        let janela = 10;
        let entrada = matches!(o.natureza, Natureza::Entrada);
        let conta_certa = o
            .conta
            .as_deref()
            .and_then(|a| self.conta_do_apelido(a))
            .map(str::to_owned);
        let esperado = self
            .composicao_vigente(&o.id)
            .map(|c| c.liquido())
            .or(o.previsto);
        let texto = ass.map(|a| normalizar_texto(&a.texto));
        let mut v: Vec<MovimentoCandidato> = self
            .extrato
            .iter()
            .filter(|m| (m.valor.0 > 0) == entrada && m.valor.0 != 0)
            .filter(|m| o.vencimento.dias_ate(m.data).abs() <= janela)
            .map(|m| MovimentoCandidato {
                bate_assinatura: texto
                    .as_ref()
                    .is_some_and(|t| normalizar_texto(&m.descricao).contains(t)),
                mesma_conta: conta_certa.as_deref() == Some(m.conta_do_extrato.as_str()),
                diferenca: esperado.map(|e| Centavos((e.0 - m.valor.0.abs()).abs())),
                movimento: m.clone(),
            })
            .collect();
        v.sort_by_key(|c| {
            (
                !c.bate_assinatura,
                !c.mesma_conta,
                o.vencimento.dias_ate(c.movimento.data).abs(),
                c.movimento.movimento.clone(),
            )
        });
        v
    }

    fn composicao_vigente(&self, id: &IdOcorrencia) -> Option<&sbf_planejamento::Composicao> {
        self.composicoes
            .iter()
            .filter(|c| c.recorrente == id.recorrente && c.desde <= id.competencia)
            .max_by_key(|c| c.desde)
    }

    pub(super) fn candidatos_impl(&self, id: &IdOcorrencia) -> Vec<MovimentoCandidato> {
        self.ocorrencia_de(&id.recorrente, id.competencia, self.hoje)
            .map(|o| self.candidatos_de(&o))
            .unwrap_or_default()
    }

    pub(super) fn buscar_impl(&mut self, periodo: &Periodo, hoje: Data) -> Resultado<usize> {
        let linhas = self.controle(periodo, Filtro::Todos, None, hoje)?.linhas;
        let mut gravadas = 0;
        for o in linhas {
            let livre = self.conciliacoes.iter().all(|c| c.ocorrencia != o.id);
            if !livre || o.pago_informado.is_some() || o.conciliado.is_some() {
                continue;
            }
            let usados: Vec<String> = self
                .conciliacoes
                .iter()
                .map(|c| c.movimento.clone())
                .collect();
            let janela_da_assinatura = self
                .assinatura_de(&o.id.recorrente)
                .map_or(10, |a| i64::from(a.janela_dias));
            let melhor = self.candidatos_de(&o).into_iter().find(|c| {
                !usados.contains(&c.movimento.movimento)
                    && o.vencimento.dias_ate(c.movimento.data).abs() <= janela_da_assinatura
                    && (c.bate_assinatura || (c.mesma_conta && c.diferenca == Some(Centavos(0))))
            });
            if let Some(c) = melhor {
                self.conciliacoes.push(Conciliacao {
                    ocorrencia: o.id.clone(),
                    movimento: c.movimento.movimento,
                    valor: Centavos(c.movimento.valor.0.abs()),
                    data: c.movimento.data,
                    confirmada: false,
                });
                gravadas += 1;
            }
        }
        Ok(gravadas)
    }

    pub(super) fn mapear_conta(
        &mut self,
        apelido: String,
        conta: Option<String>,
        autor: Autor,
        alt: &Alteracao,
    ) -> Resultado<Recibo> {
        if self.contas_impl().iter().all(|c| c.apelido != apelido) {
            return Err(ErroDoPlanejamento::NaoEncontrado(apelido));
        }
        self.mapa_contas.retain(|(a, _)| *a != apelido);
        let texto = match conta {
            Some(c) => {
                if !self.contas_do_extrato_impl().contains(&c) {
                    return Err(ErroDoPlanejamento::Invalida(
                        "essa conta não aparece no extrato".into(),
                    ));
                }
                self.mapa_contas.push((apelido, c));
                "Confirmou o par de conta."
            }
            None => "Desligou o par de conta.",
        };
        Ok(self.registrar(autor, "contas", texto.into(), alt))
    }

    pub(super) fn definir_assinatura(
        &mut self,
        recorrente: String,
        assinatura: Option<Assinatura>,
        versao_base: i64,
        autor: Autor,
        alt: &Alteracao,
    ) -> Resultado<Recibo> {
        let i = self.conferir_base(&recorrente, versao_base)?;
        if let Some(a) = &assinatura
            && (a.texto.trim().is_empty() || a.janela_dias == 0)
        {
            return Err(ErroDoPlanejamento::Invalida(
                "a assinatura precisa de texto e de uma janela em dias".into(),
            ));
        }
        self.assinaturas.retain(|(r, _)| *r != recorrente);
        let texto = if let Some(a) = assinatura {
            self.assinaturas.push((recorrente.clone(), a));
            "Definiu a assinatura."
        } else {
            "Tirou a assinatura."
        };
        self.recorrentes[i].versao_do_registro += 1;
        Ok(self.registrar(autor, &recorrente, texto.into(), alt))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::planejamento_ficticio::HOJE_DA_DEMONSTRACAO;

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn servico() -> PlanejamentoFicticio {
        PlanejamentoFicticio::novo(hoje())
    }

    fn salario_do_mes() -> IdOcorrencia {
        IdOcorrencia {
            recorrente: "rec_salario".into(),
            competencia: Competencia::de_data(hoje()),
            parcela: None,
        }
    }

    #[test]
    fn o_salario_nasce_sem_par_e_o_historico_sugere_a_conta_do_extrato() {
        let s = servico();
        let contas = s.contas().unwrap();
        let cx = contas.iter().find(|c| c.apelido == "CX-0630").expect("cx");
        assert_eq!(cx.conta_do_extrato, None);
        let nu = contas.iter().find(|c| c.apelido == "Nubank").expect("nu");
        assert_eq!(nu.conta_do_extrato.as_deref(), Some(CONTA_DO_NUBANK));
        let pares = s.pares_de_conta_sugeridos().unwrap();
        let par = pares.iter().find(|p| p.apelido == "CX-0630").expect("par");
        assert_eq!(par.conta_do_extrato, CONTA_DO_SALARIO);
        assert!(par.evidencias >= 3, "{par:?}");
        assert!(par.motivo.contains("conciliaç"), "{}", par.motivo);
        assert!(pares.iter().all(|p| p.apelido != "Nubank"));
    }

    #[test]
    fn confirmar_o_par_o_tira_das_sugestoes() {
        let mut s = servico();
        s.aplicar(
            Alteracao::MapearConta {
                apelido: "CX-0630".into(),
                conta_do_extrato: Some(CONTA_DO_SALARIO.into()),
            },
            Autor::Usuario,
        )
        .unwrap();
        let contas = s.contas().unwrap();
        let cx = contas.iter().find(|c| c.apelido == "CX-0630").unwrap();
        assert_eq!(cx.conta_do_extrato.as_deref(), Some(CONTA_DO_SALARIO));
        assert!(
            s.pares_de_conta_sugeridos()
                .unwrap()
                .iter()
                .all(|p| p.apelido != "CX-0630")
        );
        // Conta que não existe no extrato não vira par.
        let erro = s.aplicar(
            Alteracao::MapearConta {
                apelido: "CX-0630".into(),
                conta_do_extrato: Some("NADA · 1".into()),
            },
            Autor::Usuario,
        );
        assert!(erro.is_err());
        // Desligar volta ao estado sem par.
        s.aplicar(
            Alteracao::MapearConta {
                apelido: "CX-0630".into(),
                conta_do_extrato: None,
            },
            Autor::Usuario,
        )
        .unwrap();
        let contas = s.contas().unwrap();
        assert!(
            contas
                .iter()
                .find(|c| c.apelido == "CX-0630")
                .unwrap()
                .conta_do_extrato
                .is_none()
        );
    }

    #[test]
    fn candidatos_vem_da_assinatura_e_da_conta_para_o_resto() {
        let mut s = servico();
        s.aplicar(
            Alteracao::MapearConta {
                apelido: "CX-0630".into(),
                conta_do_extrato: Some(CONTA_DO_SALARIO.into()),
            },
            Autor::Usuario,
        )
        .unwrap();
        let c = s.candidatos_de_conciliacao(&salario_do_mes()).unwrap();
        assert!(c.len() >= 2, "{c:?}");
        assert!(c[0].bate_assinatura && c[0].mesma_conta, "{:?}", c[0]);
        assert_eq!(c[0].movimento.valor, LIQUIDO_DO_SALARIO);
        assert_eq!(c[0].diferenca, Some(Centavos(0)));
        // Débitos não são candidatos de uma entrada.
        assert!(c.iter().all(|x| x.movimento.valor.0 > 0));
        // Os que não batem vêm depois, com a diferença para o líquido do contracheque.
        let outro = c.iter().find(|x| !x.bate_assinatura).expect("outro");
        assert!(outro.diferenca.unwrap().0 > 0);
    }

    #[test]
    fn conciliar_manual_grava_confirmada() {
        let mut s = servico();
        let id = salario_do_mes();
        let c = s.candidatos_de_conciliacao(&id).unwrap();
        let escolhido = c
            .iter()
            .find(|x| !x.bate_assinatura)
            .unwrap()
            .movimento
            .clone();
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    ocorrencia: id.clone(),
                    movimento: escolhido.movimento.clone(),
                    valor: escolhido.valor,
                    data: escolhido.data,
                    confirmada: true,
                },
            },
            Autor::Usuario,
        )
        .unwrap();
        assert!(s.sugestao_de_conciliacao(&id).unwrap().is_none());
        let o = s
            .ocorrencia_de("rec_salario", id.competencia, hoje())
            .unwrap();
        assert_eq!(o.conciliado, Some(escolhido.valor));
    }

    #[test]
    fn assinatura_define_tira_e_confere_a_versao() {
        let mut s = servico();
        let base = s
            .recorrente("rec_mercado")
            .unwrap()
            .unwrap()
            .versao_do_registro;
        let a = Assinatura {
            texto: "MERCADO EXEMPLO".into(),
            janela_dias: 5,
        };
        s.aplicar(
            Alteracao::DefinirAssinatura {
                recorrente: "rec_mercado".into(),
                assinatura: Some(a.clone()),
                versao_base: base,
            },
            Autor::Usuario,
        )
        .unwrap();
        assert_eq!(s.assinatura("rec_mercado").unwrap(), Some(a.clone()));
        // A versão velha não grava.
        let velha = s.aplicar(
            Alteracao::DefinirAssinatura {
                recorrente: "rec_mercado".into(),
                assinatura: None,
                versao_base: base,
            },
            Autor::Usuario,
        );
        assert!(matches!(velha, Err(ErroDoPlanejamento::VersaoVelha { .. })));
        assert_eq!(s.assinatura("rec_mercado").unwrap(), Some(a));
    }

    #[test]
    fn buscar_no_extrato_sugere_so_o_que_ainda_nao_tem_conciliacao() {
        let mut s = servico();
        let id = salario_do_mes();
        // Tira a sugestão semeada para ver a busca refazê-la.
        s.conciliacoes.retain(|c| c.ocorrencia != id);
        let periodo = Periodo::MesCivil(Competencia::de_data(hoje()));
        let n = s.buscar_conciliacoes(&periodo, hoje()).unwrap();
        assert!(n >= 1, "{n}");
        let sug = s.sugestao_de_conciliacao(&id).unwrap().expect("sugestão");
        assert_eq!(sug.valor, LIQUIDO_DO_SALARIO);
        assert!(!sug.confirmada);
        // Rodar de novo não duplica.
        assert_eq!(s.buscar_conciliacoes(&periodo, hoje()).unwrap(), 0);
    }
}
