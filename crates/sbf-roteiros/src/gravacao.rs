//! A gravação guiada pela IA e o conserto proposto (RO-04).
//!
//! **Gravar.** Na primeira coleta a IA decide o próximo passo olhando a página simplificada
//! (texto visível, clicáveis com seletor, formulários; **sem** valor de campo, [`crate::pagina`]).
//! Cada proposta passa por três filtros antes de valer: o esquema JSON, a lista de bloqueio e a
//! conferência contra a página (o seletor existe, o rótulo de senha é do chaveiro, nenhum texto
//! fixo em campo de senha). Quem decide é o usuário: o passo só é executado e gravado depois
//! de aceito. Pronto o roteiro, ele roda **sem IA**.
//!
//! **Consertar.** Quando um passo falha (o site mudou), a IA recebe o roteiro, o passo que falhou
//! e a página atual, e propõe passos novos. A proposta fica à parte
//! ([`crate::banco::BancoDeRoteiros::propor_versao`]) e **só vale depois que o usuário a
//! aprova**.
//!
//! O que vai ao destinatário: instrução fixa e versionada, objetivo, descrição dos passos já
//! aceitos (sem valores), rótulos do chaveiro (nunca a senha) e a página no nível de confiança
//! do destinatário. O registro do que saiu ([`sbf_ia::Saida`]) guarda só tamanho e custo.

use crate::banco::{BancoDeRoteiros, OrigemDaVersao};
use crate::bloqueio::{self, Candidato};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::executor::{
    Contexto, Decisao, Fim, Relatorio, executar_guardando_segredos, executar_passo,
};
use crate::pagina::Pagina;
use crate::passo::{Passo, Roteiro};
use sbf_chaveiro::Segredo;
use sbf_ia::{
    Degrau, ErroDeIa, NivelDeConfianca, Pedido, Provedor, Registro, Saida, agora_rfc3339,
    validar_esquema,
};
use serde_json::{Value, json};

/// A versão do prompt de gravação (entra no registro e no cache).
pub const PROMPT_DE_GRAVACAO: &str = "roteiro-gravar-v1";
/// A versão do prompt de conserto.
pub const PROMPT_DE_CONSERTO: &str = "roteiro-consertar-v1";

const REGRAS_COMUNS: &str = "Você ajuda a montar um roteiro que NAVEGA num site para CONSULTAR e BAIXAR documentos \
(extratos, comprovantes, informes). Regras que não têm exceção: \
(1) nunca pagar, transferir, enviar PIX ou TED, contratar, investir, resgatar, sacar, comprar nem alterar \
cadastro, senha ou dados da conta: nunca proponha clicar em nada assim; \
(2) você nunca vê senhas nem valores de campos: para entrar, use preencher_cpf e preencher_senha (com um dos \
rótulos listados), nunca preencher com texto; nunca invente senha; \
(3) se a página pede código de verificação, MFA, CAPTCHA ou confirmação de identidade, responda acao=pausa; \
(4) use somente seletores que aparecem na página recebida; \
(5) responda apenas o JSON do esquema, sem prosa.";

/// O esquema de um passo (sem a pausa: ela é uma `acao`).
fn esquema_do_passo() -> Value {
    json!({
        "type": "object",
        "required": ["tipo"],
        "properties": {
            "tipo": {"enum": ["navegar", "esperar_seletor", "clicar", "preencher", "preencher_senha",
                              "preencher_cpf", "baixar", "ler_tabela"]},
            "url": {"type": "string", "maxLength": 500},
            "seletor": {"type": "string", "maxLength": 300},
            "texto": {"type": "string", "maxLength": 200},
            "rotulo": {"type": "string", "maxLength": 100},
            "nome": {"type": "string", "maxLength": 100},
            "segundos": {"type": "integer", "minimum": 1, "maximum": 120}
        }
    })
}

fn esquema_da_gravacao() -> Value {
    json!({
        "type": "object",
        "required": ["acao"],
        "properties": {
            "acao": {"enum": ["passo", "pausa", "fim"]},
            "explicacao": {"type": "string", "maxLength": 300},
            "passo": esquema_do_passo(),
            "motivo": {"type": "string", "maxLength": 200}
        }
    })
}

fn esquema_do_conserto() -> Value {
    json!({
        "type": "object",
        "required": ["acao"],
        "properties": {
            "acao": {"enum": ["conserto", "desistir"]},
            "explicacao": {"type": "string", "maxLength": 400},
            "passos": {"type": "array", "minItems": 1, "maxItems": 4, "items": esquema_do_passo()}
        }
    })
}

/// O que a IA propôs para o próximo passo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proposta {
    /// Um passo, com a explicação para o usuário aceitar ou recusar.
    Passo { passo: Passo, explicacao: String },
    /// A página pede MFA, CAPTCHA ou confirmação: passar a vez ao usuário.
    Pausa { motivo: String },
    /// O objetivo foi cumprido.
    Fim,
}

/// O que a IA sabe e o que ela pode usar.
pub struct Guia<'a> {
    pub provedor: &'a dyn Provedor,
    pub registro: Option<&'a dyn Registro>,
    /// Quem recebe (para `saida_ia`).
    pub destinatario: String,
    pub nivel: NivelDeConfianca,
    /// Os rótulos de senha de site do chaveiro (só os nomes).
    pub rotulos_de_senha: Vec<String>,
    pub tem_cpf: bool,
    /// Segredos que a página possa ter repetido: trocados por `***` antes de sair.
    pub segredos: Vec<Segredo>,
}

impl std::fmt::Debug for Guia<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guia")
            .field("provedor", &self.provedor.nome())
            .field("destinatario", &self.destinatario)
            .field("nivel", &self.nivel)
            .finish_non_exhaustive()
    }
}

impl Guia<'_> {
    /// Todo segredo que pode aparecer num texto que vai à IA: os que o chamador deu e os que
    /// a execução usou (`usados`).
    fn segredos_para_limpar<'s>(&'s self, usados: &'s [Segredo]) -> Vec<&'s str> {
        self.segredos
            .iter()
            .chain(usados)
            .map(Segredo::expor)
            .collect()
    }

    fn pagina_para_ia(&self, pagina: &Pagina, usados: &[Segredo]) -> String {
        let s = self.segredos_para_limpar(usados);
        let mut p = pagina.clone().limpar(&s);
        if self.nivel != NivelDeConfianca::Completo {
            p = p.mascarar();
        }
        p.para_texto()
    }

    /// A última barreira: o pedido inteiro (objetivo, passos, motivo da recusa, a página)
    /// passa pela limpeza de segredos antes de sair.
    fn limpar_pedido(&self, texto: &str, usados: &[Segredo]) -> String {
        crate::pagina::limpar_texto_com(texto, &self.segredos_para_limpar(usados))
    }

    fn pedir(&self, pedido: &Pedido) -> Resultado<Value> {
        let resposta = self.provedor.pedir(pedido);
        // O que saiu fica registrado mesmo quando o provedor falha depois de receber o pedido
        // (tempo esgotado, resposta ruim, limite do servidor). Só não conta o que nem saiu:
        // provedor indisponível e teto de gasto nosso.
        let (modelo, consumo, saiu) = match &resposta {
            Ok(r) => (r.modelo.clone(), r.consumo, true),
            Err(ErroDeIa::Indisponivel(_) | ErroDeIa::Teto) => {
                (String::new(), sbf_ia::Consumo::default(), false)
            }
            Err(_) => (String::new(), sbf_ia::Consumo::default(), true),
        };
        if saiu && let Some(reg) = self.registro {
            reg.registrar_saida(&Saida {
                destinatario: self.destinatario.clone(),
                provedor: self.provedor.nome().to_owned(),
                modelo,
                nivel: self.nivel,
                bytes_enviados: (pedido.instrucao.len() + pedido.entrada.len()) as u64,
                consumo,
                quando: agora_rfc3339(),
            })?;
        }
        validar_esquema(&pedido.esquema, &r.json)
            .map_err(|e| ErroDeRoteiro::Ia(ErroDeIa::RespostaInvalida(e)))?;
        Ok(r.json)
    }

    fn cabecalho_de_rotulos(&self) -> String {
        format!(
            "Rótulos de senha disponíveis no chaveiro: {}. CPF no chaveiro: {}.",
            if self.rotulos_de_senha.is_empty() {
                "(nenhum)".to_owned()
            } else {
                self.rotulos_de_senha.join(", ")
            },
            if self.tem_cpf { "sim" } else { "não" }
        )
    }
        let r = resposta?;

    /// Propõe o próximo passo da gravação. `recusada` leva o motivo da proposta anterior que o
    /// usuário ou os filtros recusaram.
    ///
    /// # Errors
    /// Falha do provedor, resposta fora do esquema ou proposta que não passa nos filtros.
    pub fn propor_proximo(
        &self,
        objetivo: &str,
        aceitos: &[Passo],
        pagina: &Pagina,
        recusada: Option<&str>,
    ) -> Resultado<Proposta> {
        self.propor_proximo_limpando(objetivo, aceitos, pagina, recusada, &[])
    }

    /// Como [`Self::propor_proximo`], mas limpa também os `usados` (os segredos que a execução
    /// já digitou) de tudo o que vai à IA.
    ///
    /// # Errors
    /// Falha do provedor, resposta fora do esquema ou proposta que não passa nos filtros.
    pub fn propor_proximo_limpando(
        &self,
        objetivo: &str,
        aceitos: &[Passo],
        pagina: &Pagina,
        recusada: Option<&str>,
        usados: &[Segredo],
    ) -> Resultado<Proposta> {
        let mut entrada = format!(
            "Objetivo: {objetivo}\n{}\nPassos já aceitos:\n",
            self.cabecalho_de_rotulos()
        );
        if aceitos.is_empty() {
            entrada.push_str("(nenhum)\n");
        }
        for (i, p) in aceitos.iter().enumerate() {
            entrada.push_str(&format!("{}. {}\n", i + 1, p.descricao()));
        }
        if let Some(r) = recusada {
            entrada.push_str(&format!("A proposta anterior foi recusada: {r}\n"));
        }
        entrada.push_str("\nPágina atual:\n");
        entrada.push_str(&self.pagina_para_ia(pagina, usados));
        let entrada = self.limpar_pedido(&entrada, usados);
        let pedido = Pedido {
            prompt_versao: PROMPT_DE_GRAVACAO.to_owned(),
            instrucao: format!(
                "{REGRAS_COMUNS} Responda UM próximo passo (acao=passo, com o objeto passo), ou acao=pausa \
                 (com motivo) se o usuário precisa agir no navegador, ou acao=fim se o objetivo foi cumprido."
            ),
            entrada,
            esquema: esquema_da_gravacao(),
            degrau: Degrau::Rapido,
            max_tokens_saida: 600,
        };
        let j = self.pedir(&pedido)?;
        match j["acao"].as_str() {
            Some("fim") => Ok(Proposta::Fim),
            Some("pausa") => Ok(Proposta::Pausa {
                motivo: j["motivo"]
                    .as_str()
                    .unwrap_or("o site pediu uma ação sua")
                    .to_owned(),
            }),
            Some("passo") => {
                let passo = com_host_da_pagina(passo_de(&j["passo"])?, pagina);
                conferir_proposta(&passo, pagina, &self.rotulos_de_senha)?;
                Ok(Proposta::Passo {
                    passo,
                    explicacao: j["explicacao"].as_str().unwrap_or("").to_owned(),
                })
            }
            _ => Err(ErroDeRoteiro::Invalido("a IA não disse o que fazer".into())),
        }
    }

    /// Propõe o conserto do passo `falho` do roteiro, olhando a página atual. Devolve o
    /// roteiro **inteiro** como ficaria, com a explicação. Não grava nada: quem grava como
    /// proposta é o chamador, e só vale depois da aprovação.
    ///
    /// # Errors
    /// Falha do provedor, a IA desistir, ou proposta que não passa nos filtros.
    pub fn propor_conserto(
        &self,
        roteiro: &Roteiro,
        falho: usize,
        motivo_da_falha: &str,
        pagina: &Pagina,
    ) -> Resultado<(Vec<Passo>, String)> {
        self.propor_conserto_limpando(roteiro, falho, motivo_da_falha, pagina, &[])
    }

    /// Como [`Self::propor_conserto`], mas limpa também os `usados` (os segredos que a
    /// execução digitou) de tudo o que vai à IA.
    ///
    /// # Errors
    /// Falha do provedor, a IA desistir, ou proposta que não passa nos filtros.
    pub fn propor_conserto_limpando(
        &self,
        roteiro: &Roteiro,
        falho: usize,
        motivo_da_falha: &str,
        pagina: &Pagina,
        usados: &[Segredo],
    ) -> Resultado<(Vec<Passo>, String)> {
        let mut entrada = format!(
            "{}\nRoteiro \"{}\":\n",
            self.cabecalho_de_rotulos(),
            roteiro.nome
        );
        for (i, p) in roteiro.passos.iter().enumerate() {
            let marca = if i == falho { "  <-- FALHOU" } else { "" };
            entrada.push_str(&format!("{}. {}{marca}\n", i + 1, p.descricao()));
        }
        if let Some(p) = roteiro.passos.get(falho) {
            entrada.push_str(&format!(
                "\nO passo que falhou, em JSON: {}\nMotivo: {motivo_da_falha}\n",
                serde_json::to_string(p).unwrap_or_default()
            ));
        }
        entrada.push_str("\nPágina atual:\n");
        entrada.push_str(&self.pagina_para_ia(pagina, usados));
        let entrada = self.limpar_pedido(&entrada, usados);
        let pedido = Pedido {
            prompt_versao: PROMPT_DE_CONSERTO.to_owned(),
            instrucao: format!(
                "{REGRAS_COMUNS} O site mudou e um passo do roteiro falhou. Proponha de 1 a 4 passos que \
                 SUBSTITUEM o passo que falhou (acao=conserto, com passos e explicacao), ou acao=desistir \
                 se não for possível consertar sem agir fora das regras."
            ),
            entrada,
            esquema: esquema_do_conserto(),
            degrau: Degrau::Forte,
            max_tokens_saida: 900,
        };
        let j = self.pedir(&pedido)?;
        if j["acao"].as_str() != Some("conserto") {
            return Err(ErroDeRoteiro::Invalido(
                "a IA não conseguiu propor um conserto".into(),
            ));
        }
        let novos: Vec<Passo> = j["passos"]
            .as_array()
            .ok_or_else(|| ErroDeRoteiro::Invalido("conserto sem passos".into()))?
            .iter()
            .map(|p| passo_de(p).map(|p| com_host_da_pagina(p, pagina)))
            .collect::<Resultado<_>>()?;
        for p in &novos {
            conferir_proposta(p, pagina, &self.rotulos_de_senha)?;
        }
        let mut todos: Vec<Passo> = roteiro.passos[..falho.min(roteiro.passos.len())].to_vec();
        todos.extend(novos);
        if falho < roteiro.passos.len() {
            todos.extend_from_slice(&roteiro.passos[falho + 1..]);
        }
        Ok((todos, j["explicacao"].as_str().unwrap_or("").to_owned()))
    }
}

/// Quem decide o host em que a senha vale é o SBF (a página em que o passo é proposto), nunca a
/// IA: ela pode até escrever um `host` no JSON, e ele é trocado aqui.
fn com_host_da_pagina(passo: Passo, pagina: &Pagina) -> Passo {
    let host = crate::passo::host_de(&pagina.url);
    match passo {
        Passo::PreencherSenha {
            seletor, rotulo, ..
        } => Passo::PreencherSenha {
            seletor,
            rotulo,
            host,
        },
        Passo::PreencherCpf { seletor, .. } => Passo::PreencherCpf { seletor, host },
        outro => outro,
    }
}

fn passo_de(v: &Value) -> Resultado<Passo> {
    serde_json::from_value::<Passo>(v.clone())
        .map_err(|e| ErroDeRoteiro::Invalido(format!("passo fora do formato: {e}")))
}

/// O candidato na página para o seletor, como o executor o conferiria.
fn candidato_na_pagina(pagina: &Pagina, seletor: &str) -> Option<Candidato> {
    if let Some(e) = pagina.elementos.iter().find(|e| e.seletor == seletor) {
        return Some(Candidato::com(&[
            ("texto", &e.texto),
            ("aria-label", &e.rotulo),
            ("destino", &e.destino),
        ]));
    }
    None
}

fn campo_na_pagina<'a>(pagina: &'a Pagina, seletor: &str) -> Option<&'a crate::pagina::Campo> {
    pagina
        .formularios
        .iter()
        .flat_map(|f| &f.campos)
        .find(|c| c.seletor == seletor)
}

/// O elemento tem texto ou rótulo (aria-label, title, alt) que se possa ler? Campos contam
/// pelo rótulo ou pelo nome.
fn tem_nome_acessivel(pagina: &Pagina, seletor: &str) -> bool {
    if let Some(e) = pagina.elementos.iter().find(|e| e.seletor == seletor) {
        return !e.texto.trim().is_empty() || !e.rotulo.trim().is_empty();
    }
    if let Some(c) = campo_na_pagina(pagina, seletor) {
        return !c.rotulo.trim().is_empty() || !c.nome.trim().is_empty();
    }
    true
}

/// Os filtros que toda proposta da IA passa antes de chegar ao usuário.
fn conferir_proposta(passo: &Passo, pagina: &Pagina, rotulos: &[String]) -> Resultado<()> {
    let recusa = |m: &str| Err(ErroDeRoteiro::Invalido(m.to_owned()));
    let existe = |s: &str| {
        pagina.elementos.iter().any(|e| e.seletor == s)
            || campo_na_pagina(pagina, s).is_some()
            || pagina.tabelas.iter().any(|t| t.seletor == s)
            || pagina.formularios.iter().any(|f| f.seletor == s)
    };
    match passo {
        Passo::Navegar { url } => {
            if let Some(b) = bloqueio::avaliar_url(url) {
                return Err(ErroDeRoteiro::Bloqueado(b));
            }
            Ok(())
        }
        Passo::Clicar { seletor } | Passo::Baixar { seletor, .. } => {
            if !existe(seletor) {
                return recusa("o seletor não está na página");
            }
            if let Some(c) = candidato_na_pagina(pagina, seletor)
                && let Some(b) = bloqueio::avaliar(&c)
            {
                return Err(ErroDeRoteiro::Bloqueado(b));
            }
            // Um elemento sem texto nem rótulo é um alvo que o usuário não consegue conferir
            // (um link invisível, um ícone sem nome): a IA não pode propor clicar nele.
            if !tem_nome_acessivel(pagina, seletor) {
                return recusa("o elemento não tem texto nem rótulo que o usuário possa conferir");
            }
            Ok(())
        }
        Passo::Preencher { seletor, .. } => {
            let Some(c) = campo_na_pagina(pagina, seletor) else {
                return recusa("o campo não está na página");
            };
            if c.tipo == "password" {
                return recusa("campo de senha só se preenche com a senha do chaveiro");
            }
            Ok(())
        }
        Passo::PreencherSenha {
            seletor, rotulo, ..
        } => {
            let Some(c) = campo_na_pagina(pagina, seletor) else {
                return recusa("o campo não está na página");
            };
            if c.tipo != "password" {
                return recusa("a senha do chaveiro só vai num campo de senha (type=password)");
            }
            if !rotulos.iter().any(|r| r == rotulo) {
                return recusa("esse rótulo não existe no chaveiro");
            }
            if !crate::passo::esquema_aceito_para_segredo(&pagina.url) {
                return recusa("a senha do chaveiro só vai para uma página https");
            }
            Ok(())
        }
        Passo::PreencherCpf { seletor, .. } => {
            let Some(c) = campo_na_pagina(pagina, seletor) else {
                return recusa("o campo não está na página");
            };
            if !matches!(c.tipo.as_str(), "text" | "tel" | "number" | "search") {
                return recusa("o CPF só vai num campo de texto");
            }
            if !crate::passo::esquema_aceito_para_segredo(&pagina.url) {
                return recusa("o CPF do chaveiro só vai para uma página https");
            }
            Ok(())
        }
        Passo::LerTabela { seletor, .. } => {
            if existe(seletor) {
                Ok(())
            } else {
                recusa("o seletor não está na página")
            }
        }
        Passo::EsperarSeletor { .. } => Ok(()),
        Passo::PausaParaUsuario { .. } => recusa("a pausa se propõe com acao=pausa"),
    }
}

/// O que o usuário decide sobre uma proposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aprovacao {
    /// Executa o passo e o grava no roteiro.
    Aceitar,
    /// Não executa; o motivo volta para a IA.
    Recusar(String),
    /// Termina a gravação aqui.
    Parar,
}

/// Quem decide: a janela, ou um teste.
pub trait Aprovador {
    fn decidir(&mut self, proposta: &Proposta) -> Aprovacao;
}

/// Por que a gravação parou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FimDaGravacao {
    /// A IA disse que o objetivo foi cumprido.
    Cumprido,
    /// O usuário parou.
    Parou,
    /// Chegou ao limite de passos.
    Limite,
    /// O usuário não voltou de uma pausa.
    PausaNaoResolvida,
}

/// O resultado de uma gravação guiada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gravado {
    pub passos: Vec<Passo>,
    pub fim: FimDaGravacao,
}

/// Grava um roteiro com a IA. Cada passo aceito é **executado** no navegador antes de pedir o
/// próximo, e só então entra no roteiro. Uma proposta recusada pelos filtros (seletor que não
/// existe, ação perigosa, rótulo inventado) volta para a IA com o motivo; depois de três
/// recusas seguidas a gravação desiste com erro.
///
/// # Errors
/// Falha do provedor, três propostas inválidas seguidas, ação bloqueada ao executar, ou
/// falha de banco ou disco.
pub fn gravar_guiado(
    guia: &Guia<'_>,
    c: &mut Contexto<'_>,
    execucao: i64,
    objetivo: &str,
    url_inicial: &str,
    aprovador: &mut dyn Aprovador,
    max_passos: usize,
) -> Resultado<Gravado> {
    let mut aceitos: Vec<Passo> = Vec::new();
    let mut usados: Vec<Segredo> = Vec::new();
    let mut rel = Relatorio {
        fim: Fim::Concluida,
        baixados: 0,
        novos: 0,
        pausas: 0,
    };
    let mut recusada: Option<String> = None;
    let mut seguidas = 0_u32;
    let mut liberado_em: Option<usize> = None;

    if !url_inicial.is_empty() {
        let p = Passo::Navegar {
            url: url_inicial.to_owned(),
        };
        executar_passo(c, execucao, 0, &p, &mut usados, &mut rel)?;
        aceitos.push(p);
    }
    let fim = loop {
        if aceitos.len() >= max_passos {
            break FimDaGravacao::Limite;
        }
        let pagina = c.navegador.pagina()?;
        if liberado_em != Some(aceitos.len())
            && let Some(m) = pagina.pausa_necessaria()
        {
            if pausar_na_gravacao(c, execucao, aceitos.len(), m.frase())? != Decisao::Continuar {
                break FimDaGravacao::PausaNaoResolvida;
            }
            aceitos.push(Passo::PausaParaUsuario {
                motivo: m.frase().to_owned(),
            });
            liberado_em = Some(aceitos.len());
            continue;
        }
        let proposta = match guia.propor_proximo_limpando(
            objetivo,
            &aceitos,
            &pagina,
            recusada.as_deref(),
            &usados,
        ) {
            Ok(p) => p,
            Err(ErroDeRoteiro::Invalido(m)) => {
                recusada = Some(m);
                seguidas += 1;
                if seguidas >= 3 {
                    return Err(ErroDeRoteiro::Invalido(
                        "a IA fez três propostas seguidas que não servem".into(),
                    ));
                }
                continue;
            }
            Err(ErroDeRoteiro::Bloqueado(b)) => {
                recusada = Some(format!("bloqueada pela lista de segurança: {b}"));
                seguidas += 1;
                if seguidas >= 3 {
                    return Err(ErroDeRoteiro::Bloqueado(b));
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        seguidas = 0;
        recusada = None;
        match &proposta {
            Proposta::Fim => break FimDaGravacao::Cumprido,
            Proposta::Pausa { motivo } => {
                match aprovador.decidir(&proposta) {
                    Aprovacao::Parar => break FimDaGravacao::Parou,
                    Aprovacao::Recusar(m) => {
                        recusada = Some(m);
                        continue;
                    }
                    Aprovacao::Aceitar => {}
                }
                if pausar_na_gravacao(c, execucao, aceitos.len(), motivo)? != Decisao::Continuar {
                    break FimDaGravacao::PausaNaoResolvida;
                }
                aceitos.push(Passo::PausaParaUsuario {
                    motivo: motivo.clone(),
                });
                liberado_em = Some(aceitos.len());
            }
            Proposta::Passo { passo, .. } => match aprovador.decidir(&proposta) {
                Aprovacao::Parar => break FimDaGravacao::Parou,
                Aprovacao::Recusar(m) => recusada = Some(m),
                Aprovacao::Aceitar => {
                    match executar_passo(c, execucao, aceitos.len(), passo, &mut usados, &mut rel) {
                        Ok(()) => aceitos.push(passo.clone()),
                        Err(ErroDeRoteiro::PassoFalhou { motivo, .. }) => {
                            recusada = Some(format!("o passo falhou ao executar: {motivo}"));
                        }
                        Err(e) => return Err(e),
                    }
                }
            },
        }
    };
    drop(usados);
    Ok(Gravado {
        passos: aceitos,
        fim,
    })
}

fn pausar_na_gravacao(
    c: &mut Contexto<'_>,
    execucao: i64,
    passo: usize,
    motivo: &str,
) -> Resultado<Decisao> {
    c.banco.marcar_aguardando(execucao, motivo)?;
    c.banco.evento(execucao, Some(passo), "pausa", motivo)?;
    let d = c.pausa.aguardar(execucao, motivo);
    if d == Decisao::Continuar {
        c.banco.marcar_rodando(execucao)?;
    }
    Ok(d)
}

/// Executa o roteiro e, se um passo falhar com o site mudado, pede à IA (se houver) um
/// conserto e o guarda como **proposta**. O roteiro ativo não muda: a proposta só vale depois
/// que o usuário a aprova ([`BancoDeRoteiros::aprovar_proposta`]).
///
/// # Errors
/// Só falha de infraestrutura; a falha do passo e a do conserto ficam no [`Relatorio`] e nos
/// eventos.
pub fn executar_com_conserto(
    c: &mut Contexto<'_>,
    roteiro_id: i64,
    execucao: i64,
    roteiro: &Roteiro,
    guia: Option<&Guia<'_>>,
) -> Resultado<Relatorio> {
    let mut usados: Vec<Segredo> = Vec::new();
    let rel = executar_guardando_segredos(c, execucao, roteiro, &mut usados)?;
    if let (Fim::Falhou { passo, motivo }, Some(g)) = (&rel.fim, guia) {
        let banco: &BancoDeRoteiros = c.banco;
        let pagina = c.navegador.pagina().unwrap_or_default();
        // Os segredos que a execução digitou saem de tudo o que vai à IA.
        match g.propor_conserto_limpando(roteiro, *passo, motivo, &pagina, &usados) {
            Ok((passos, explicacao)) => {
                banco.propor_versao(roteiro_id, &passos, &explicacao, OrigemDaVersao::Conserto)?;
                banco.evento(
                    execucao,
                    Some(*passo),
                    "conserto",
                    "a IA propôs um conserto; aguarda a sua aprovação",
                )?;
            }
            Err(e) => {
                banco.evento(
                    execucao,
                    Some(*passo),
                    "conserto",
                    &format!("sem conserto: {e}"),
                )?;
            }
        }
    }
    Ok(rel)
}
