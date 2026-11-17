//! Implementação falsa e mínima do trait `Planejamento`, só para testes. Não use fora deles.

use std::collections::HashMap;

use sbf_planejamento::*;

#[derive(Debug, Default)]
pub struct Falso {
    pub recorrentes: Vec<Recorrente>,
    pub grupos: Vec<Grupo>,
    pub origens: HashMap<OrigemImportada, String>,
    pub excecoes: Vec<(IdOcorrencia, Excecao)>,
    pub conciliacoes: Vec<Conciliacao>,
    pub aplicadas: Vec<(Alteracao, Autor)>,
    /// O que `controle` devolve nas linhas.
    pub ocorrencias: Vec<Ocorrencia>,
    pub composicoes: Vec<Composicao>,
    pub documentos: Vec<DocumentoVinculado>,
    /// As contas com o par (o que `contas` devolve).
    pub contas: Vec<ContaDoPlanejamento>,
    /// A assinatura por recorrente.
    pub assinaturas: HashMap<String, Assinatura>,
}

fn nao_suportado<T>() -> Resultado<T> {
    Err(ErroDoPlanejamento::Invalida(
        "o falso não faz isso".to_owned(),
    ))
}

impl Planejamento for Falso {
    fn controle(
        &self,
        periodo: &Periodo,
        _: Filtro,
        _: Option<&str>,
        _: Data,
    ) -> Resultado<Controle> {
        Ok(Controle {
            periodo: *periodo,
            grupos: self.grupos.clone(),
            linhas: self.ocorrencias.clone(),
            atrasadas_de_antes: Vec::new(),
            resumo: Resumo::default(),
        })
    }

    fn recorrentes(&self, incluir_excluidos: bool) -> Resultado<Vec<Recorrente>> {
        Ok(self
            .recorrentes
            .iter()
            .filter(|r| incluir_excluidos || r.excluido.is_none())
            .cloned()
            .collect())
    }

    fn recorrente(&self, id: &str) -> Resultado<Option<Recorrente>> {
        Ok(self.recorrentes.iter().find(|r| r.id == id).cloned())
    }

    fn grupos(&self) -> Resultado<Vec<Grupo>> {
        Ok(self.grupos.clone())
    }

    /// Só a ocorrência de `desde`, com o estado `Adiada` se há exceção `Adiar` nela.
    fn proximas(
        &self,
        recorrente: &str,
        desde: Competencia,
        _: u16,
        _: Data,
    ) -> Resultado<Vec<Ocorrencia>> {
        let r = self
            .recorrentes
            .iter()
            .find(|r| r.id == recorrente)
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(recorrente.to_owned()))?;
        let id = IdOcorrencia {
            recorrente: r.id.clone(),
            competencia: desde,
            parcela: None,
        };
        let adiada = self
            .excecoes
            .iter()
            .any(|(o, e)| *o == id && matches!(e, Excecao::Adiar { .. }));
        let v = &r.versoes[0];
        Ok(vec![Ocorrencia {
            id,
            nome: r.nome.clone(),
            grupo: r.grupo.clone(),
            subgrupo: None,
            posicao: r.posicao,
            natureza: r.natureza,
            vencimento: desde.dia(1),
            vencimento_original: desde.dia(1),
            previsto: v.valor,
            certeza: v.certeza,
            fonte: v.fonte,
            pago_informado: None,
            conciliado: None,
            estado: if adiada {
                EstadoDaOcorrencia::Adiada
            } else {
                EstadoDaOcorrencia::Prevista
            },
            parcela: None,
            conta: None,
            centro: None,
            fornecedor: None,
            versao: v.id.clone(),
            prazo_desconhecido: false,
            sugestao_de_conciliacao: false,
        }])
    }

    fn calendario(&self, _: Competencia, _: u16, _: Data) -> Resultado<Vec<TotaisDoMes>> {
        nao_suportado()
    }

    fn simular(&self, _: &Alteracao, _: Data) -> Resultado<Simulacao> {
        nao_suportado()
    }

    fn aplicar(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo> {
        let n = self.aplicadas.len() + 1;
        let mut alvo = format!("alvo{n}");
        let mut versao_nova = 1;
        match &alteracao {
            Alteracao::CriarRecorrente {
                recorrente: r,
                origem,
            } => {
                if let Some(o) = origem
                    && self.origens.contains_key(o)
                {
                    return Err(ErroDoPlanejamento::Invalida("origem repetida".to_owned()));
                }
                let id = format!("rec{n}");
                let mut v = r.primeira_versao.clone();
                v.id = format!("v{n}");
                self.recorrentes.push(Recorrente {
                    id: id.clone(),
                    nome: r.nome.clone(),
                    grupo: r.grupo.clone(),
                    subgrupo: r.subgrupo.clone(),
                    posicao: r.posicao,
                    natureza: r.natureza,
                    prazo: r.prazo,
                    versoes: vec![v],
                    pausas: Vec::new(),
                    cancelamento: None,
                    arquivado: false,
                    excluido: None,
                    versao_do_registro: 1,
                });
                if let Some(o) = origem {
                    self.origens.insert(o.clone(), id.clone());
                }
                alvo = id;
            }
            Alteracao::CriarGrupo { nome, posicao } => {
                self.grupos.push(Grupo {
                    nome: nome.clone(),
                    posicao: *posicao,
                });
            }
            Alteracao::Excecao {
                ocorrencia,
                excecao,
            } => {
                self.excecoes.push((ocorrencia.clone(), excecao.clone()));
            }
            Alteracao::Conciliar { conciliacao } => {
                self.conciliacoes.push(conciliacao.clone());
            }
            Alteracao::Editar {
                recorrente,
                escopo: Escopo::DaquiEmDiante(c),
                mudanca,
                versao_base,
            } => {
                let r = self
                    .recorrentes
                    .iter_mut()
                    .find(|r| &r.id == recorrente)
                    .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(recorrente.clone()))?;
                if r.versao_do_registro != *versao_base {
                    return Err(ErroDoPlanejamento::VersaoVelha {
                        atual: r.versao_do_registro,
                    });
                }
                let mut v = r.versoes.last().cloned().expect("versao");
                v.id = format!("v{n}");
                v.desde = *c;
                if let Some(valor) = mudanca.valor {
                    v.valor = valor;
                }
                if let Some(c) = mudanca.certeza {
                    v.certeza = c;
                }
                if let Some(f) = mudanca.fonte {
                    v.fonte = f;
                }
                r.versoes.push(v);
                r.versao_do_registro += 1;
                versao_nova = r.versao_do_registro;
                alvo = r.id.clone();
            }
            Alteracao::DefinirComposicao { composicao, .. } => {
                alvo = composicao.recorrente.clone();
                self.composicoes.retain(|c| {
                    !(c.recorrente == composicao.recorrente && c.desde == composicao.desde)
                });
                self.composicoes.push(composicao.clone());
            }
            Alteracao::VincularDocumento {
                alvo: a,
                sha256,
                tipo_documento,
                nome,
                data,
            } => {
                self.documentos.push(DocumentoVinculado {
                    id: format!("doc{n}"),
                    alvo: a.clone(),
                    sha256: sha256.clone(),
                    tipo: *tipo_documento,
                    nome: nome.clone(),
                    data: *data,
                    quando: "2026-11-16T00:00:00Z".to_owned(),
                    autor: autor.clone(),
                    desvinculado: false,
                });
            }
            _ => return nao_suportado(),
        }
        self.aplicadas.push((alteracao, autor));
        Ok(Recibo {
            alteracao_id: format!("alt{n}"),
            alvo,
            versao_nova,
            quando: "2026-11-16T00:00:00Z".to_owned(),
        })
    }

    fn propor(&mut self, _: Alteracao, _: &str, _: &str) -> Resultado<Proposta> {
        nao_suportado()
    }

    fn propostas(&self, _: bool) -> Resultado<Vec<Proposta>> {
        Ok(Vec::new())
    }

    fn confirmar_proposta(&mut self, _: &str) -> Resultado<Recibo> {
        nao_suportado()
    }

    fn recusar_proposta(&mut self, _: &str, _: &str) -> Resultado<()> {
        nao_suportado()
    }

    fn historico(&self, _: Option<&str>, _: u32) -> Resultado<Vec<EntradaDoHistorico>> {
        Ok(Vec::new())
    }

    fn composicao(
        &self,
        recorrente: &str,
        competencia: Competencia,
    ) -> Resultado<Option<Composicao>> {
        Ok(self
            .composicoes
            .iter()
            .filter(|c| c.recorrente == recorrente && c.desde <= competencia)
            .max_by_key(|c| c.desde)
            .cloned())
    }

    fn documentos(&self, alvo: &AlvoDoDocumento) -> Resultado<Vec<DocumentoVinculado>> {
        Ok(self
            .documentos
            .iter()
            .filter(|d| &d.alvo == alvo && !d.desvinculado)
            .cloned()
            .collect())
    }

    fn contas(&self) -> Resultado<Vec<ContaDoPlanejamento>> {
        Ok(self.contas.clone())
    }

    fn assinatura(&self, recorrente: &str) -> Resultado<Option<Assinatura>> {
        Ok(self.assinaturas.get(recorrente).cloned())
    }

    fn origem_importada(&self, origem: &OrigemImportada) -> Resultado<Option<String>> {
        Ok(self.origens.get(origem).cloned())
    }
}
