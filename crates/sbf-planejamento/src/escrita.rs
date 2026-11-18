//! A escrita: toda alteração aceita passa por [`aplicar_na`], que roda dentro da transação
//! de quem chama (o serviço, a confirmação de proposta ou a simulação) e grava uma linha
//! no histórico. Nada aqui apaga de fato: pausa, cancelamento, exceção e exclusão são
//! linhas, e `desfeita` é uma marca.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::banco::{self, carregar_um, de_json, json, novo_id};
use crate::projecao::{projetar_competencias, versao_vigente};
use crate::{
    Alteracao, AlvoDoDocumento, Assinatura, Autor, Cancelamento, Competencia, Composicao,
    Conciliacao, Data, ErroDoPlanejamento, Escopo, Excecao, Exclusao, Frequencia, IdOcorrencia,
    MudancaDeVersao, NovoRecorrente, OrigemImportada, Prazo, Recibo, Recorrente, Resultado,
    TipoDeDocumento, Versao,
};

/// O que uma variante fez, para o histórico e o recibo.
struct Feito {
    alvo: String,
    recorrente: Option<String>,
    resumo: String,
    versao_nova: i64,
    /// `false` quando nada mudou (a mesma origem importada duas vezes).
    grava_historico: bool,
    /// O que desfaz esta alteração (D15), guardado junto da linha do histórico.
    inversa: Option<Inversa>,
}

/// O inverso de uma alteração, em JSON na tabela `inversas`. Quase sempre é outra
/// [`Alteracao`]; pausa e cancelamento não têm alteração própria que os desfaça, então
/// ganham um inverso interno.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub(crate) enum Inversa {
    Alteracao {
        alteracao: Box<Alteracao>,
    },
    /// Fixa o fim de uma pausa (`ate = desde` a esvazia; o valor de antes a reabre).
    AjustarPausa {
        recorrente: String,
        pausa: String,
        ate: Option<Competencia>,
    },
    /// Volta o cancelamento ao de antes, ou a nenhum.
    Cancelamento {
        recorrente: String,
        anterior: Option<Cancelamento>,
    },
}

fn inversa_de(alteracao: Alteracao) -> Option<Inversa> {
    Some(Inversa::Alteracao {
        alteracao: Box::new(alteracao),
    })
}

fn invalida<T>(motivo: &str) -> Resultado<T> {
    Err(ErroDoPlanejamento::Invalida(motivo.to_owned()))
}

/// Aplica uma alteração dentro da transação de `c` e deixa a linha no histórico.
pub(crate) fn aplicar_na(
    c: &Connection,
    alteracao: &Alteracao,
    autor: &Autor,
    agora: &str,
) -> Resultado<Recibo> {
    let feito = match alteracao {
        Alteracao::CriarRecorrente { recorrente, origem } => {
            criar_recorrente(c, recorrente, origem.as_ref(), agora)?
        }
        Alteracao::Editar {
            recorrente,
            escopo,
            mudanca,
            versao_base,
        } => editar(c, recorrente, *escopo, mudanca, *versao_base, autor, agora)?,
        Alteracao::Pausar {
            recorrente,
            desde,
            ate,
            motivo,
            versao_base,
        } => pausar(c, recorrente, *desde, *ate, motivo, *versao_base)?,
        Alteracao::Retomar {
            recorrente,
            desde,
            versao_base,
        } => retomar(c, recorrente, *desde, *versao_base)?,
        Alteracao::Cancelar {
            recorrente,
            desde,
            motivo,
            comprovado,
            versao_base,
        } => cancelar(c, recorrente, *desde, motivo, *comprovado, *versao_base)?,
        Alteracao::Excecao {
            ocorrencia,
            excecao,
        } => excecao_avulsa(c, ocorrencia, excecao, autor, agora)?,
        Alteracao::DesfazerExcecao { id } => desfazer_excecao(c, id)?,
        Alteracao::Conciliar { conciliacao } => conciliar(c, conciliacao, autor)?,
        Alteracao::Excluir {
            recorrente,
            motivo,
            versao_base,
        } => excluir(c, recorrente, motivo, *versao_base, autor, agora)?,
        Alteracao::Restaurar {
            recorrente,
            versao_base,
        } => restaurar(c, recorrente, *versao_base)?,
        Alteracao::Arquivar {
            recorrente,
            arquivado,
            versao_base,
        } => arquivar(c, recorrente, *arquivado, *versao_base)?,
        Alteracao::Renomear {
            recorrente,
            nome,
            versao_base,
        } => renomear(c, recorrente, nome, *versao_base)?,
        Alteracao::Mover {
            recorrente,
            grupo,
            posicao,
            versao_base,
        } => mover(c, recorrente, grupo, *posicao, *versao_base)?,
        Alteracao::CriarGrupo { nome, posicao } => criar_grupo(c, nome, *posicao)?,
        Alteracao::DefinirComposicao {
            composicao,
            versao_base,
        } => definir_composicao(c, composicao, *versao_base, autor, agora)?,
        Alteracao::VincularDocumento {
            alvo,
            sha256,
            tipo_documento,
            nome,
            data,
        } => vincular_documento(c, alvo, sha256, *tipo_documento, nome, *data, autor, agora)?,
        Alteracao::DesvincularDocumento { id } => desvincular_documento(c, id)?,
        Alteracao::MapearConta {
            apelido,
            conta_do_extrato,
        } => mapear_conta(c, apelido, conta_do_extrato.as_deref(), autor, agora)?,
        Alteracao::DefinirAssinatura {
            recorrente,
            assinatura,
            versao_base,
        } => definir_assinatura(
            c,
            recorrente,
            assinatura.as_ref(),
            *versao_base,
            autor,
            agora,
        )?,
        Alteracao::Desconciliar {
            ocorrencia,
            movimento,
        } => desconciliar(c, ocorrencia, movimento)?,
    };

    fechar(c, feito, &json(alteracao)?, autor, agora)
}

/// Grava a linha do histórico (com o inverso, se houver) e devolve o recibo.
fn fechar(
    c: &Connection,
    feito: Feito,
    alteracao_json: &str,
    autor: &Autor,
    agora: &str,
) -> Resultado<Recibo> {
    let alteracao_id = if feito.grava_historico {
        let id = novo_id(c, "hist")?;
        c.execute(
            "INSERT INTO historico (id, quando, autor, alvo, recorrente_id, resumo, alteracao)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                agora,
                json(autor)?,
                feito.alvo,
                feito.recorrente,
                feito.resumo,
                alteracao_json
            ],
        )?;
        if let Some(inversa) = &feito.inversa {
            c.execute(
                "INSERT INTO inversas (historico_id, inversa) VALUES (?1, ?2)",
                params![id, json(inversa)?],
            )?;
        }
        id
    } else {
        String::new()
    };
    Ok(Recibo {
        alteracao_id,
        alvo: feito.alvo,
        versao_nova: feito.versao_nova,
        quando: agora.to_owned(),
    })
}

// ---------------------------------------------------------------------------------------
// Desfazer (D15)
// ---------------------------------------------------------------------------------------

/// Põe a versão do registro de hoje nas alterações que a exigem: o inverso foi guardado
/// quando o registro estava em outra versão.
fn com_base_atual(c: &Connection, alteracao: Alteracao) -> Resultado<Alteracao> {
    let atual = |id: &str| carregar(c, id).map(|r| r.versao_do_registro);
    Ok(match alteracao {
        Alteracao::Editar {
            recorrente,
            escopo,
            mudanca,
            ..
        } => Alteracao::Editar {
            versao_base: atual(&recorrente)?,
            recorrente,
            escopo,
            mudanca,
        },
        Alteracao::Excluir {
            recorrente, motivo, ..
        } => Alteracao::Excluir {
            versao_base: atual(&recorrente)?,
            recorrente,
            motivo,
        },
        Alteracao::Restaurar { recorrente, .. } => Alteracao::Restaurar {
            versao_base: atual(&recorrente)?,
            recorrente,
        },
        Alteracao::Renomear {
            recorrente, nome, ..
        } => Alteracao::Renomear {
            versao_base: atual(&recorrente)?,
            recorrente,
            nome,
        },
        outra => outra,
    })
}

/// Desfaz a alteração `id` do histórico: aplica o inverso guardado, grava a linha do Desfazer
/// e liga as duas. Dentro da transação de quem chama.
pub(crate) fn desfazer_na(
    c: &Connection,
    id: &str,
    autor: &Autor,
    agora: &str,
) -> Resultado<Recibo> {
    let existe_no_historico =
        banco::existe(c, "SELECT 1 FROM historico WHERE id = ?1", params![id])?;
    if !existe_no_historico {
        return Err(ErroDoPlanejamento::NaoEncontrado(format!("alteração {id}")));
    }
    if banco::existe(
        c,
        "SELECT 1 FROM desfeitos WHERE original_id = ?1",
        params![id],
    )? {
        return invalida("essa alteração já foi desfeita");
    }
    if banco::existe(
        c,
        "SELECT 1 FROM desfeitos WHERE desfazedor_id = ?1",
        params![id],
    )? {
        return invalida("isso já é um Desfazer: refaça a alteração, se quiser");
    }
    let inversa: Option<String> = c
        .query_row(
            "SELECT inversa FROM inversas WHERE historico_id = ?1",
            params![id],
            |l| l.get(0),
        )
        .optional()?;
    let Some(inversa) = inversa else {
        return invalida("essa alteração não tem como ser desfeita automaticamente");
    };
    let inversa: Inversa = de_json(&inversa)?;
    let recibo = match &inversa {
        Inversa::Alteracao { alteracao } => {
            let alteracao = com_base_atual(c, (**alteracao).clone())?;
            aplicar_na(c, &alteracao, autor, agora)?
        }
        Inversa::AjustarPausa {
            recorrente,
            pausa,
            ate,
        } => {
            let rec = carregar(c, recorrente)?;
            c.execute(
                "UPDATE pausas SET ate = ?2 WHERE id = ?1 AND recorrente_id = ?3",
                params![pausa, ate.map(|a| a.to_string()), recorrente],
            )?;
            let versao_nova = subir_versao(c, recorrente)?;
            let feito = Feito {
                alvo: rec.id.clone(),
                recorrente: Some(rec.id),
                resumo: "Desfez uma pausa ou retomada.".into(),
                versao_nova,
                grava_historico: true,
                inversa: None,
            };
            fechar(c, feito, &json(&inversa)?, autor, agora)?
        }
        Inversa::Cancelamento {
            recorrente,
            anterior,
        } => {
            carregar(c, recorrente)?;
            match anterior {
                Some(a) => {
                    c.execute(
                        "UPDATE cancelamentos SET desde = ?2, motivo = ?3, comprovado = ?4,
                                desfeito = 0 WHERE recorrente_id = ?1",
                        params![recorrente, a.desde.to_string(), a.motivo, a.comprovado],
                    )?;
                }
                None => {
                    c.execute(
                        "UPDATE cancelamentos SET desfeito = 1 WHERE recorrente_id = ?1",
                        params![recorrente],
                    )?;
                }
            }
            let versao_nova = subir_versao(c, recorrente)?;
            let feito = Feito {
                alvo: recorrente.clone(),
                recorrente: Some(recorrente.clone()),
                resumo: "Desfez um cancelamento.".into(),
                versao_nova,
                grava_historico: true,
                inversa: None,
            };
            fechar(c, feito, &json(&inversa)?, autor, agora)?
        }
    };
    c.execute(
        "INSERT INTO desfeitos (original_id, desfazedor_id, quando) VALUES (?1, ?2, ?3)",
        params![id, recibo.alteracao_id, agora],
    )?;
    Ok(recibo)
}

// ---------------------------------------------------------------------------------------
// Validações
// ---------------------------------------------------------------------------------------

fn validar_frequencia(f: &Frequencia) -> Resultado<()> {
    let dia_ok = |d: u8| (1..=31).contains(&d);
    let valida = match *f {
        Frequencia::Mensal { dia } => dia_ok(dia),
        Frequencia::CadaNMeses { n, dia, .. } => n >= 1 && dia_ok(dia),
        Frequencia::Anual { mes, dia } => (1..=12).contains(&mes) && dia_ok(dia),
        Frequencia::Unica { .. } => true,
    };
    if valida {
        Ok(())
    } else {
        invalida("frequência inválida")
    }
}

fn validar_prazo(p: &Prazo) -> Resultado<()> {
    match *p {
        Prazo::Parcelas { total: 0, .. } => {
            invalida("parcelas: o total precisa ser maior que zero")
        }
        _ => Ok(()),
    }
}

fn nao_vazio(texto: &str, campo: &str) -> Resultado<()> {
    if texto.trim().is_empty() {
        invalida(&format!("{campo} não pode ficar vazio"))
    } else {
        Ok(())
    }
}

fn carregar(c: &Connection, id: &str) -> Resultado<Recorrente> {
    banco::ler_recorrentes(c, Some(id), true)?
        .into_iter()
        .next()
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("recorrente {id}")))
}

/// O recorrente existe, não está excluído e a versão de base confere.
fn carregar_com_base(c: &Connection, id: &str, base: i64) -> Resultado<Recorrente> {
    let rec = carregar(c, id)?;
    conferir_base(&rec, base)?;
    if rec.excluido.is_some() {
        return invalida("o recorrente está excluído; restaure antes");
    }
    Ok(rec)
}

fn conferir_base(rec: &Recorrente, base: i64) -> Resultado<()> {
    if rec.versao_do_registro == base {
        Ok(())
    } else {
        Err(ErroDoPlanejamento::VersaoVelha {
            atual: rec.versao_do_registro,
        })
    }
}

/// Sobe a versão do registro e devolve a nova.
fn subir_versao(c: &Connection, id: &str) -> Resultado<i64> {
    Ok(c.query_row(
        "UPDATE recorrentes SET versao_do_registro = versao_do_registro + 1
         WHERE id = ?1 RETURNING versao_do_registro",
        params![id],
        |l| l.get(0),
    )?)
}

fn garantir_grupo(c: &Connection, nome: &str) -> Resultado<()> {
    c.execute(
        "INSERT OR IGNORE INTO grupos (nome, posicao)
         VALUES (?1, COALESCE((SELECT MAX(posicao) FROM grupos), 0) + 1)",
        params![nome],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Variantes
// ---------------------------------------------------------------------------------------

fn gravar_versao(c: &Connection, recorrente: &str, v: &Versao, nova: bool) -> Resultado<()> {
    let valores = params![
        v.id,
        recorrente,
        v.desde.to_string(),
        v.valor.map(|x| x.0),
        json(&v.certeza)?,
        json(&v.fonte)?,
        json(&v.frequencia)?,
        v.conta,
        v.centro,
        v.fornecedor,
        v.categoria,
        v.nota
    ];
    if nova {
        c.execute(
            "INSERT INTO versoes (id, recorrente_id, desde, valor, certeza, fonte, frequencia,
                                  conta, centro, fornecedor, categoria, nota)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            valores,
        )?;
    } else {
        c.execute(
            "UPDATE versoes SET valor = ?4, certeza = ?5, fonte = ?6, frequencia = ?7,
                    conta = ?8, centro = ?9, fornecedor = ?10, categoria = ?11, nota = ?12
             WHERE id = ?1 AND recorrente_id = ?2 AND desde = ?3",
            valores,
        )?;
    }
    Ok(())
}

fn criar_recorrente(
    c: &Connection,
    novo: &NovoRecorrente,
    origem: Option<&OrigemImportada>,
    agora: &str,
) -> Resultado<Feito> {
    if let Some(o) = origem {
        let existente: Option<String> = c
            .query_row(
                "SELECT recorrente_id FROM origens_importadas
                 WHERE sha256 = ?1 AND aba = ?2 AND linha = ?3",
                params![o.arquivo_sha256, o.aba, o.linha],
                |l| l.get(0),
            )
            .optional()?;
        if let Some(id) = existente {
            let rec = carregar(c, &id)?;
            return Ok(Feito {
                alvo: id,
                recorrente: None,
                resumo: String::new(),
                versao_nova: rec.versao_do_registro,
                grava_historico: false,
                inversa: None,
            });
        }
    }
    nao_vazio(&novo.nome, "o nome")?;
    nao_vazio(&novo.grupo, "o grupo")?;
    validar_prazo(&novo.prazo)?;
    validar_frequencia(&novo.primeira_versao.frequencia)?;

    let id = novo_id(c, "rec")?;
    garantir_grupo(c, &novo.grupo)?;
    c.execute(
        "INSERT INTO recorrentes (id, nome, grupo, subgrupo, posicao, natureza, prazo,
                                  versao_do_registro, criado_em)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
        params![
            id,
            novo.nome,
            novo.grupo,
            novo.subgrupo,
            novo.posicao,
            json(&novo.natureza)?,
            json(&novo.prazo)?,
            agora
        ],
    )?;
    let versao = Versao {
        id: novo_id(c, "ver")?,
        ..novo.primeira_versao.clone()
    };
    gravar_versao(c, &id, &versao, true)?;
    if let Some(o) = origem {
        c.execute(
            "INSERT INTO origens_importadas (sha256, aba, linha, recorrente_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![o.arquivo_sha256, o.aba, o.linha, id],
        )?;
    }
    Ok(Feito {
        alvo: id.clone(),
        recorrente: Some(id.clone()),
        resumo: "Criou o recorrente.".into(),
        versao_nova: 1,
        grava_historico: true,
        inversa: inversa_de(Alteracao::Excluir {
            recorrente: id,
            motivo: "criação desfeita".into(),
            versao_base: 0,
        }),
    })
}

fn aplicar_mudanca(v: &mut Versao, m: &MudancaDeVersao) {
    if let Some(valor) = m.valor {
        v.valor = valor;
    }
    if let Some(certeza) = m.certeza {
        v.certeza = certeza;
    }
    if let Some(fonte) = m.fonte {
        v.fonte = fonte;
    }
    if let Some(frequencia) = m.frequencia {
        v.frequencia = frequencia;
    }
    if let Some(x) = &m.conta {
        v.conta.clone_from(x);
    }
    if let Some(x) = &m.centro {
        v.centro.clone_from(x);
    }
    if let Some(x) = &m.fornecedor {
        v.fornecedor.clone_from(x);
    }
    if let Some(x) = &m.categoria {
        v.categoria.clone_from(x);
    }
    if let Some(x) = &m.nota {
        v.nota.clone_from(x);
    }
}

fn campos_mudados(m: &MudancaDeVersao) -> Vec<&'static str> {
    let mut campos = Vec::new();
    let mut marca = |tem: bool, nome: &'static str| {
        if tem {
            campos.push(nome);
        }
    };
    marca(m.valor.is_some(), "valor");
    marca(m.certeza.is_some(), "certeza");
    marca(m.fonte.is_some(), "fonte");
    marca(m.frequencia.is_some(), "frequência");
    marca(m.conta.is_some(), "conta");
    marca(m.centro.is_some(), "centro");
    marca(m.fornecedor.is_some(), "fornecedor");
    marca(m.categoria.is_some(), "categoria");
    marca(m.nota.is_some(), "nota");
    campos
}

fn editar(
    c: &Connection,
    id: &str,
    escopo: Escopo,
    mudanca: &MudancaDeVersao,
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = carregar_com_base(c, id, base)?;
    let campos = campos_mudados(mudanca);
    if campos.is_empty() {
        return invalida("nenhuma mudança");
    }
    let (resumo, inversa) = match escopo {
        Escopo::SoEsteMes(competencia) => {
            let so_valor = campos == ["valor"];
            let Some(Some(valor)) = mudanca.valor.filter(|_| so_valor) else {
                return invalida("só o valor muda só neste mês, e ele precisa ter valor");
            };
            let ocorrencia = id_da_ocorrencia(c, id, competencia)?;
            let exc = registrar_excecao(
                c,
                &ocorrencia,
                &Excecao::ValorSoDesteMes { valor },
                autor,
                agora,
            )?;
            (
                format!("Mudou o valor só de {competencia}."),
                inversa_de(Alteracao::DesfazerExcecao { id: exc }),
            )
        }
        Escopo::DaquiEmDiante(competencia) => {
            if let Some(f) = &mudanca.frequencia {
                validar_frequencia(f)?;
            }
            let base_da_versao = versao_vigente(&rec, competencia)
                .or_else(|| rec.versoes.first())
                .ok_or_else(|| ErroDoPlanejamento::Banco("recorrente sem versão".into()))?;
            let mesma_data = base_da_versao.desde == competencia;
            let restauracao = restaurar_campos(mudanca, base_da_versao);
            let mut nova = base_da_versao.clone();
            aplicar_mudanca(&mut nova, mudanca);
            nova.desde = competencia;
            if !mesma_data {
                nova.id = novo_id(c, "ver")?;
            }
            gravar_versao(c, id, &nova, !mesma_data)?;
            (
                format!(
                    "Nova versão a partir de {competencia} ({}).",
                    campos.join(", ")
                ),
                inversa_de(Alteracao::Editar {
                    recorrente: id.to_owned(),
                    escopo: Escopo::DaquiEmDiante(competencia),
                    mudanca: restauracao,
                    versao_base: 0,
                }),
            )
        }
    };
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo,
        versao_nova,
        grava_historico: true,
        inversa,
    })
}

/// A mudança que devolve os campos de `m` aos valores de `v` (o que valia antes).
fn restaurar_campos(m: &MudancaDeVersao, v: &Versao) -> MudancaDeVersao {
    MudancaDeVersao {
        valor: m.valor.map(|_| v.valor),
        certeza: m.certeza.map(|_| v.certeza),
        fonte: m.fonte.map(|_| v.fonte),
        frequencia: m.frequencia.map(|_| v.frequencia),
        conta: m.conta.as_ref().map(|_| v.conta.clone()),
        centro: m.centro.as_ref().map(|_| v.centro.clone()),
        fornecedor: m.fornecedor.as_ref().map(|_| v.fornecedor.clone()),
        categoria: m.categoria.as_ref().map(|_| v.categoria.clone()),
        nota: m.nota.as_ref().map(|_| v.nota.clone()),
    }
}

/// O id (com a parcela) da ocorrência do recorrente naquela competência.
fn id_da_ocorrencia(
    c: &Connection,
    recorrente: &str,
    competencia: Competencia,
) -> Resultado<IdOcorrencia> {
    let (rec, excecoes, conciliacoes) = carregar_um(c, recorrente)?
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("recorrente {recorrente}")))?;
    // O `hoje` só decide o estado; o id não depende dele.
    let qualquer_dia = Data {
        ano: 2000,
        mes: 1,
        dia: 1,
    };
    projetar_competencias(&rec, &excecoes, &conciliacoes, [competencia], qualquer_dia)
        .into_iter()
        .next()
        .map(|o| o.id)
        .ok_or_else(|| ErroDoPlanejamento::Invalida(format!("não há ocorrência em {competencia}")))
}

fn tipo_da_excecao(e: &Excecao) -> &'static str {
    match e {
        Excecao::Adiar { .. } => "adiar",
        Excecao::Dispensar { .. } => "dispensar",
        Excecao::ValorSoDesteMes { .. } => "valor_so_deste_mes",
        Excecao::PagoInformado { .. } => "pago_informado",
    }
}

/// Grava a exceção; a anterior do mesmo tipo na mesma ocorrência fica `desfeita`
/// (a última decisão vale, e desfazer não ressuscita a antiga).
fn registrar_excecao(
    c: &Connection,
    ocorrencia: &IdOcorrencia,
    excecao: &Excecao,
    autor: &Autor,
    agora: &str,
) -> Resultado<String> {
    let texto = ocorrencia.to_string();
    let tipo = tipo_da_excecao(excecao);
    c.execute(
        "UPDATE excecoes SET desfeita = 1 WHERE ocorrencia = ?1 AND tipo = ?2 AND desfeita = 0",
        params![texto, tipo],
    )?;
    let id = novo_id(c, "exc")?;
    c.execute(
        "INSERT INTO excecoes (id, recorrente_id, ocorrencia, competencia, parcela, tipo,
                               excecao, quando, autor, desfeita)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
        params![
            id,
            ocorrencia.recorrente,
            texto,
            ocorrencia.competencia.to_string(),
            ocorrencia.parcela,
            tipo,
            json(excecao)?,
            agora,
            json(autor)?
        ],
    )?;
    Ok(id)
}

fn pausar(
    c: &Connection,
    id: &str,
    desde: Competencia,
    ate: Option<Competencia>,
    motivo: &str,
    base: i64,
) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    nao_vazio(motivo, "o motivo")?;
    if ate.is_some_and(|a| a <= desde) {
        return invalida("a pausa precisa terminar depois de começar");
    }
    if banco::existe(
        c,
        "SELECT 1 FROM pausas WHERE recorrente_id = ?1 AND ate IS NULL",
        params![id],
    )? {
        return invalida("já há uma pausa aberta; retome antes");
    }
    let pausa_id = novo_id(c, "pau")?;
    c.execute(
        "INSERT INTO pausas (id, recorrente_id, desde, ate, motivo) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            pausa_id,
            id,
            desde.to_string(),
            ate.map(|a| a.to_string()),
            motivo
        ],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: format!("Pausou a partir de {desde}."),
        versao_nova,
        grava_historico: true,
        // Uma pausa que acaba onde começa não suprime mês nenhum.
        inversa: Some(Inversa::AjustarPausa {
            recorrente: id.to_owned(),
            pausa: pausa_id,
            ate: Some(desde),
        }),
    })
}

fn retomar(c: &Connection, id: &str, desde: Competencia, base: i64) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    let pausa: Option<(String, Option<String>)> = c
        .query_row(
            "SELECT id, ate FROM pausas
             WHERE recorrente_id = ?1 AND desde < ?2 AND (ate IS NULL OR ate > ?2)
             ORDER BY desde DESC LIMIT 1",
            params![id, desde.to_string()],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?;
    let Some((pausa, ate_antes)) = pausa else {
        return invalida("não há pausa para retomar nesta competência");
    };
    let ate_antes = ate_antes
        .as_deref()
        .map(banco::ler_competencia)
        .transpose()?;
    c.execute(
        "UPDATE pausas SET ate = ?2 WHERE id = ?1",
        params![pausa, desde.to_string()],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: format!("Retomou em {desde}."),
        versao_nova,
        grava_historico: true,
        inversa: Some(Inversa::AjustarPausa {
            recorrente: id.to_owned(),
            pausa,
            ate: ate_antes,
        }),
    })
}

fn cancelar(
    c: &Connection,
    id: &str,
    desde: Competencia,
    motivo: &str,
    comprovado: bool,
    base: i64,
) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    nao_vazio(motivo, "o motivo")?;
    let anterior = carregar(c, id)?.cancelamento;
    c.execute(
        "INSERT INTO cancelamentos (recorrente_id, desde, motivo, comprovado)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (recorrente_id) DO UPDATE
         SET desde = excluded.desde, motivo = excluded.motivo, comprovado = excluded.comprovado,
             desfeito = 0",
        params![id, desde.to_string(), motivo, comprovado],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: format!(
            "Cancelou a partir de {desde}{}.",
            if comprovado { " (comprovado)" } else { "" }
        ),
        versao_nova,
        grava_historico: true,
        inversa: Some(Inversa::Cancelamento {
            recorrente: id.to_owned(),
            anterior,
        }),
    })
}

/// O recorrente da ocorrência existe e não está excluído.
fn recorrente_da_ocorrencia(c: &Connection, ocorrencia: &IdOcorrencia) -> Resultado<Recorrente> {
    let rec = carregar(c, &ocorrencia.recorrente)?;
    if rec.excluido.is_some() {
        return invalida("o recorrente está excluído; restaure antes");
    }
    Ok(rec)
}

fn excecao_avulsa(
    c: &Connection,
    ocorrencia: &IdOcorrencia,
    excecao: &Excecao,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = recorrente_da_ocorrencia(c, ocorrencia)?;
    if let Excecao::Dispensar { motivo } = excecao {
        nao_vazio(motivo, "o motivo")?;
    }
    let exc = registrar_excecao(c, ocorrencia, excecao, autor, agora)?;
    let resumo = match excecao {
        Excecao::Adiar { nova_data: Some(d) } => format!("Adiou {ocorrencia} para {d}."),
        Excecao::Adiar { nova_data: None } => format!("Adiou {ocorrencia} sem nova data."),
        Excecao::Dispensar { .. } => format!("Dispensou {ocorrencia}."),
        Excecao::ValorSoDesteMes { .. } => {
            format!("Mudou o valor só de {}.", ocorrencia.competencia)
        }
        Excecao::PagoInformado { .. } => format!("Informou pago em {ocorrencia}."),
    };
    Ok(Feito {
        alvo: ocorrencia.to_string(),
        recorrente: Some(rec.id),
        resumo,
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: inversa_de(Alteracao::DesfazerExcecao { id: exc }),
    })
}

fn desfazer_excecao(c: &Connection, id: &str) -> Resultado<Feito> {
    let linha: Option<(String, String)> = c
        .query_row(
            "SELECT ocorrencia, recorrente_id FROM excecoes WHERE id = ?1",
            params![id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?;
    let Some((ocorrencia, recorrente)) = linha else {
        return Err(ErroDoPlanejamento::NaoEncontrado(format!("exceção {id}")));
    };
    c.execute(
        "UPDATE excecoes SET desfeita = 1 WHERE id = ?1",
        params![id],
    )?;
    let rec = carregar(c, &recorrente)?;
    Ok(Feito {
        alvo: ocorrencia.clone(),
        recorrente: Some(recorrente),
        resumo: format!("Desfez uma exceção de {ocorrencia}."),
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: None,
    })
}

fn conciliar(c: &Connection, x: &Conciliacao, autor: &Autor) -> Resultado<Feito> {
    let rec = recorrente_da_ocorrencia(c, &x.ocorrencia)?;
    nao_vazio(&x.movimento, "o movimento")?;
    // Uma sugestão nunca rebaixa uma conciliação já confirmada; uma desfeita volta a valer
    // (como sugestão ou confirmada) quando alguém grava o par de novo.
    c.execute(
        "INSERT INTO conciliacoes (ocorrencia, recorrente_id, competencia, parcela, movimento,
                                   valor, data, confirmada, autor)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (ocorrencia, movimento) DO UPDATE
         SET valor = excluded.valor, data = excluded.data, confirmada = excluded.confirmada,
             autor = excluded.autor, desfeita = 0
         WHERE conciliacoes.desfeita = 1 OR conciliacoes.confirmada = 0
            OR excluded.confirmada = 1",
        params![
            x.ocorrencia.to_string(),
            x.ocorrencia.recorrente,
            x.ocorrencia.competencia.to_string(),
            x.ocorrencia.parcela,
            x.movimento,
            x.valor.0,
            x.data.to_string(),
            x.confirmada,
            json(autor)?
        ],
    )?;
    Ok(Feito {
        alvo: x.ocorrencia.to_string(),
        recorrente: Some(rec.id),
        resumo: if x.confirmada {
            format!("Confirmou a conciliação de {}.", x.ocorrencia)
        } else {
            format!("Sugeriu a conciliação de {}.", x.ocorrencia)
        },
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: inversa_de(Alteracao::Desconciliar {
            ocorrencia: x.ocorrencia.clone(),
            movimento: x.movimento.clone(),
        }),
    })
}

/// Desfaz a conciliação (confirmada ou sugerida) do par: exclusão lógica. A linha fica, marcada
/// `desfeita`, para o histórico e para uma regra não refazer sozinha o que o usuário desfez.
fn desconciliar(c: &Connection, ocorrencia: &IdOcorrencia, movimento: &str) -> Resultado<Feito> {
    let rec = recorrente_da_ocorrencia(c, ocorrencia)?;
    let antes: Option<(i64, String, bool)> = c
        .query_row(
            "SELECT valor, data, confirmada FROM conciliacoes
             WHERE ocorrencia = ?1 AND movimento = ?2 AND desfeita = 0",
            params![ocorrencia.to_string(), movimento],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .optional()?;
    let tocadas = c.execute(
        "UPDATE conciliacoes SET desfeita = 1, sem_automatico = 1
         WHERE ocorrencia = ?1 AND movimento = ?2 AND desfeita = 0",
        params![ocorrencia.to_string(), movimento],
    )?;
    if tocadas == 0 {
        return Err(ErroDoPlanejamento::NaoEncontrado(format!(
            "conciliação de {ocorrencia}"
        )));
    }
    Ok(Feito {
        alvo: ocorrencia.to_string(),
        recorrente: Some(rec.id),
        resumo: format!("Desfez a conciliação de {ocorrencia}."),
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: match antes {
            Some((valor, data, confirmada)) => inversa_de(Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    ocorrencia: ocorrencia.clone(),
                    movimento: movimento.to_owned(),
                    valor: crate::Centavos(valor),
                    data: banco::ler_data(&data)?,
                    confirmada,
                },
            }),
            None => None,
        },
    })
}

fn excluir(
    c: &Connection,
    id: &str,
    motivo: &str,
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    nao_vazio(motivo, "o motivo")?;
    let exclusao = Exclusao {
        quando: agora.to_owned(),
        autor: autor.clone(),
        motivo: motivo.to_owned(),
    };
    c.execute(
        "UPDATE recorrentes SET excluido = ?2 WHERE id = ?1",
        params![id, json(&exclusao)?],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: "Excluiu (exclusão lógica).".into(),
        versao_nova,
        grava_historico: true,
        inversa: inversa_de(Alteracao::Restaurar {
            recorrente: id.to_owned(),
            versao_base: 0,
        }),
    })
}

fn restaurar(c: &Connection, id: &str, base: i64) -> Resultado<Feito> {
    let rec = carregar(c, id)?;
    conferir_base(&rec, base)?;
    if rec.excluido.is_none() {
        return invalida("o recorrente não está excluído");
    }
    c.execute(
        "UPDATE recorrentes SET excluido = NULL WHERE id = ?1",
        params![id],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: "Restaurou o recorrente.".into(),
        versao_nova,
        grava_historico: true,
        inversa: inversa_de(Alteracao::Excluir {
            recorrente: id.to_owned(),
            motivo: "restauração desfeita".into(),
            versao_base: 0,
        }),
    })
}

fn arquivar(c: &Connection, id: &str, arquivado: bool, base: i64) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    c.execute(
        "UPDATE recorrentes SET arquivado = ?2 WHERE id = ?1",
        params![id, arquivado],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: if arquivado {
            "Arquivou o recorrente.".into()
        } else {
            "Tirou o recorrente do arquivo.".into()
        },
        versao_nova,
        grava_historico: true,
        inversa: None,
    })
}

fn renomear(c: &Connection, id: &str, nome: &str, base: i64) -> Resultado<Feito> {
    let rec = carregar_com_base(c, id, base)?;
    let nome = nome.trim();
    nao_vazio(nome, "o nome")?;
    c.execute(
        "UPDATE recorrentes SET nome = ?2 WHERE id = ?1",
        params![id, nome],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: "Renomeou o compromisso.".into(),
        versao_nova,
        grava_historico: true,
        inversa: inversa_de(Alteracao::Renomear {
            recorrente: id.to_owned(),
            nome: rec.nome,
            versao_base: 0,
        }),
    })
}

fn mover(c: &Connection, id: &str, grupo: &str, posicao: i64, base: i64) -> Resultado<Feito> {
    carregar_com_base(c, id, base)?;
    nao_vazio(grupo, "o grupo")?;
    garantir_grupo(c, grupo)?;
    c.execute(
        "UPDATE recorrentes SET grupo = ?2, posicao = ?3 WHERE id = ?1",
        params![id, grupo, posicao],
    )?;
    let versao_nova = subir_versao(c, id)?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(id.to_owned()),
        resumo: "Mudou o grupo ou a prioridade.".into(),
        versao_nova,
        grava_historico: true,
        inversa: None,
    })
}

fn criar_grupo(c: &Connection, nome: &str, posicao: i64) -> Resultado<Feito> {
    nao_vazio(nome, "o nome do grupo")?;
    c.execute(
        "INSERT INTO grupos (nome, posicao) VALUES (?1, ?2)
         ON CONFLICT (nome) DO UPDATE SET posicao = excluded.posicao",
        params![nome, posicao],
    )?;
    Ok(Feito {
        alvo: nome.to_owned(),
        recorrente: None,
        resumo: "Criou ou reordenou um grupo.".into(),
        versao_nova: 0,
        grava_historico: true,
        inversa: None,
    })
}

// ---------------------------------------------------------------------------------------
// Composição do salário e documentos
// ---------------------------------------------------------------------------------------

/// Grava as rubricas a partir de `desde`; a de mesmo `desde` é substituída no lugar. Não
/// muda a versão do registro nem o valor digitado.
fn definir_composicao(
    c: &Connection,
    composicao: &Composicao,
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = carregar_com_base(c, &composicao.recorrente, base)?;
    nao_vazio(&composicao.origem, "a origem")?;
    if composicao.rubricas.iter().any(|r| r.nome.trim().is_empty()) {
        return invalida("toda rubrica precisa de nome");
    }
    let desde = composicao.desde.to_string();
    let rubricas = json(&composicao.rubricas)?;
    let atualizadas = c.execute(
        "UPDATE composicoes SET rubricas = ?3, documento = ?4, origem = ?5, quando = ?6,
                autor = ?7
         WHERE recorrente_id = ?1 AND desde = ?2",
        params![
            rec.id,
            desde,
            rubricas,
            composicao.documento,
            composicao.origem,
            agora,
            json(autor)?
        ],
    )?;
    if atualizadas == 0 {
        c.execute(
            "INSERT INTO composicoes (id, recorrente_id, desde, rubricas, documento, origem,
                                      quando, autor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                novo_id(c, "comp")?,
                rec.id,
                desde,
                rubricas,
                composicao.documento,
                composicao.origem,
                agora,
                json(autor)?
            ],
        )?;
    }
    Ok(Feito {
        alvo: rec.id.clone(),
        recorrente: Some(rec.id),
        resumo: format!(
            "Definiu a composição do salário desde {} ({} rubricas).",
            composicao.desde,
            composicao.rubricas.len()
        ),
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn vincular_documento(
    c: &Connection,
    alvo: &AlvoDoDocumento,
    sha256: &str,
    tipo: TipoDeDocumento,
    nome: &str,
    data: Option<Data>,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    nao_vazio(sha256, "o sha256")?;
    let (recorrente, ocorrencia) = match alvo {
        AlvoDoDocumento::Recorrente { id } => (id.clone(), None),
        AlvoDoDocumento::Ocorrencia { id } => (id.recorrente.clone(), Some(id.to_string())),
    };
    let rec = carregar(c, &recorrente)?;
    let existente: Option<String> = c
        .query_row(
            "SELECT id FROM documentos_vinculados
             WHERE recorrente_id = ?1 AND ocorrencia IS ?2 AND sha256 = ?3 AND desvinculado = 0",
            params![recorrente, ocorrencia, sha256],
            |l| l.get(0),
        )
        .optional()?;
    if let Some(id) = existente {
        return Ok(Feito {
            alvo: id,
            recorrente: None,
            resumo: String::new(),
            versao_nova: rec.versao_do_registro,
            grava_historico: false,
            inversa: None,
        });
    }
    nao_vazio(nome, "o nome do documento")?;
    let id = novo_id(c, "doc")?;
    c.execute(
        "INSERT INTO documentos_vinculados
             (id, recorrente_id, ocorrencia, sha256, tipo, nome, data, quando, autor)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            recorrente,
            ocorrencia,
            sha256,
            json(&tipo)?,
            nome,
            data.map(|d| d.to_string()),
            agora,
            json(autor)?
        ],
    )?;
    Ok(Feito {
        alvo: id,
        recorrente: Some(recorrente),
        resumo: "Vinculou um documento.".into(),
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: None,
    })
}

/// Marca o documento como desvinculado; desvincular de novo não faz nada.
fn desvincular_documento(c: &Connection, id: &str) -> Resultado<Feito> {
    let (recorrente, desvinculado): (String, bool) = c
        .query_row(
            "SELECT recorrente_id, desvinculado FROM documentos_vinculados WHERE id = ?1",
            params![id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("documento {id}")))?;
    let rec = carregar(c, &recorrente)?;
    if desvinculado {
        return Ok(Feito {
            alvo: id.to_owned(),
            recorrente: None,
            resumo: String::new(),
            versao_nova: rec.versao_do_registro,
            grava_historico: false,
            inversa: None,
        });
    }
    c.execute(
        "UPDATE documentos_vinculados SET desvinculado = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(Feito {
        alvo: id.to_owned(),
        recorrente: Some(recorrente),
        resumo: "Desvinculou um documento.".into(),
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: None,
    })
}

// ---------------------------------------------------------------------------------------
// Contas do extrato e assinaturas
// ---------------------------------------------------------------------------------------

/// A maior janela de uma assinatura, em dias.
const JANELA_MAXIMA_DA_ASSINATURA: u8 = 60;

/// Confirma (ou desfaz, com `None`) o par apelido ↔ conta do extrato. Nada é apagado: desfazer
/// deixa a linha sem conta.
fn mapear_conta(
    c: &Connection,
    apelido: &str,
    conta_do_extrato: Option<&str>,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let apelido = apelido.trim();
    nao_vazio(apelido, "o apelido da conta")?;
    let conta = conta_do_extrato.map(str::trim);
    if let Some(conta) = conta {
        nao_vazio(conta, "a conta do extrato")?;
    }
    c.execute(
        "INSERT INTO contas_do_planejamento (apelido, conta_do_extrato, quando, autor)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (apelido) DO UPDATE
         SET conta_do_extrato = excluded.conta_do_extrato, quando = excluded.quando,
             autor = excluded.autor",
        params![apelido, conta, agora, json(autor)?],
    )?;
    Ok(Feito {
        alvo: apelido.to_owned(),
        recorrente: None,
        resumo: if conta.is_some() {
            format!("Confirmou a conta do extrato de {apelido}.")
        } else {
            format!("Desfez o par de conta de {apelido}.")
        },
        versao_nova: 0,
        grava_historico: true,
        inversa: None,
    })
}

/// Define (ou tira, com `None`) a assinatura de um recorrente. Confere a versão de base como
/// as demais regras do recorrente; não muda a projeção, então não sobe a versão.
fn definir_assinatura(
    c: &Connection,
    recorrente: &str,
    assinatura: Option<&Assinatura>,
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = carregar_com_base(c, recorrente, base)?;
    let (texto, janela) = match assinatura {
        Some(a) => {
            if crate::conciliacao::normalizar(&a.texto).trim().is_empty() {
                return invalida("o texto da assinatura não pode ficar vazio");
            }
            if a.janela_dias == 0 || a.janela_dias > JANELA_MAXIMA_DA_ASSINATURA {
                return invalida("a janela da assinatura vai de 1 a 60 dias");
            }
            (Some(a.texto.trim()), Some(i64::from(a.janela_dias)))
        }
        None => (None, None),
    };
    c.execute(
        "INSERT INTO assinaturas (recorrente_id, texto, janela_dias, quando, autor)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (recorrente_id) DO UPDATE
         SET texto = excluded.texto, janela_dias = excluded.janela_dias,
             quando = excluded.quando, autor = excluded.autor",
        params![rec.id, texto, janela, agora, json(autor)?],
    )?;
    Ok(Feito {
        alvo: rec.id.clone(),
        recorrente: Some(rec.id),
        resumo: if assinatura.is_some() {
            "Definiu a assinatura do extrato.".to_owned()
        } else {
            "Tirou a assinatura do extrato.".to_owned()
        },
        versao_nova: rec.versao_do_registro,
        grava_historico: true,
        inversa: None,
    })
}
