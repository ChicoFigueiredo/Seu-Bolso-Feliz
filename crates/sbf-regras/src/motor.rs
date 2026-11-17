//! O motor: casa as versões de regra com uma [`Entrada`] e monta a [`Leitura`].
//!
//! Para cada versão cuja `aplica_a` inclui o tipo da entrada: detectar → extrair → (sem
//! leitor) script → obrigatórios → propostas. Um erro de regex ou de script numa versão
//! vira uma [`Execucao`] com resultado `Erro` e **não derruba as outras**: um script errado
//! não trava a fila.
//!
//! ## Precedência
//!
//! Vence a de maior `prioridade`; no empate, a de maior especificidade
//! ([`Definicao::especificidade`]). Se as melhores empatam nos dois e **discordam**
//! (tipo documental diferente, ou um campo crítico com valor diferente nas duas:
//! `valor_centavos`, `data`, `vencimento`, `conta`, `contrato`, `autenticacao`), a decisão
//! é [`Decisao::Ambiguo`] e nada é gravado (AC-LUA-03). Se empatam e concordam, vence a de
//! menor id (regra, versão), com o motivo dito. O vencedor leva em `motivos` por que
//! venceu (AC-LUA-02).
//!
//! ## Determinismo
//!
//! Mesma entrada, mesmas versões e mesmos parâmetros dão a mesma [`Leitura`], byte a byte
//! na serialização (AC-LUA-04), com **uma exceção**: `Execucao::duracao_us` é medida do
//! relógio; é o único campo não determinístico e não decide nada. [`sem_duracao`] o zera
//! para comparar.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value as Json, json};

use crate::catalogo::{Catalogo, Parametro, VersaoDeRegra, parametro_vigente, sha256_hex};
use crate::definicao::{Compilada, normalizar};
use crate::script::{ExecutorDeScript, Limites, PedidoDeScript, RespostaDoScript};
use crate::{
    Campo, DeOnde, Decisao, Entrada, ErroDasRegras, Execucao, Leitura, Reconhecimento,
    ResultadoDaExecucao, Valor,
};

/// Os campos que, se duas regras empatadas discordam neles, tornam a leitura ambígua.
pub const CAMPOS_CRITICOS: [&str; 6] = [
    "valor_centavos",
    "data",
    "vencimento",
    "conta",
    "contrato",
    "autenticacao",
];

/// A chave de idempotência: sha256 hex do JSON de
/// `[entrada.id, regra, versão, checksum, [checksums dos parâmetros usados]]`.
pub fn chave_de_idempotencia(
    entrada_id: &str,
    regra: &str,
    versao: u32,
    checksum: &str,
    checksums_dos_parametros: &[String],
) -> String {
    let base = json!([
        entrada_id,
        regra,
        versao,
        checksum,
        checksums_dos_parametros
    ]);
    sha256_hex(base.to_string().as_bytes())
}

/// A leitura com `duracao_us` zerado: o que se compara entre execuções repetidas.
pub fn sem_duracao(leitura: &Leitura) -> Leitura {
    let mut l = leitura.clone();
    for e in &mut l.execucoes {
        e.duracao_us = 0;
    }
    l
}

#[derive(Debug)]
struct VersaoCompilada {
    versao: VersaoDeRegra,
    compilada: Result<Compilada, String>,
    especificidade: usize,
}

struct Candidata {
    prioridade: i32,
    especificidade: usize,
    rec: Reconhecimento,
}

/// O motor de regras, sobre um conjunto fixo de versões e parâmetros.
#[derive(Debug)]
pub struct Motor {
    versoes: Vec<VersaoCompilada>,
    parametros: Vec<Parametro>,
    executor: Option<Arc<dyn ExecutorDeScript>>,
}

impl Motor {
    /// Um motor com estas versões (qualquer estado: quem chama escolhe) e parâmetros.
    pub fn novo(
        versoes: Vec<VersaoDeRegra>,
        parametros: Vec<Parametro>,
        executor: Option<Arc<dyn ExecutorDeScript>>,
    ) -> Self {
        let mut versoes: Vec<VersaoCompilada> = versoes
            .into_iter()
            .map(|v| VersaoCompilada {
                compilada: v.definicao.compilar().map_err(|e| e.to_string()),
                especificidade: v.definicao.especificidade(),
                versao: v,
            })
            .collect();
        versoes.sort_by(|a, b| {
            (&a.versao.regra, a.versao.versao).cmp(&(&b.versao.regra, b.versao.versao))
        });
        Self {
            versoes,
            parametros,
            executor,
        }
    }

    /// O motor das versões ativas e consolidadas do catálogo.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn das_vigentes(
        catalogo: &Catalogo,
        executor: Option<Arc<dyn ExecutorDeScript>>,
    ) -> Result<Self, ErroDasRegras> {
        Ok(Self::novo(
            catalogo.vigentes()?,
            catalogo.parametros()?,
            executor,
        ))
    }

    /// Lê uma entrada com o executor do motor.
    pub fn ler(&self, entrada: &Entrada) -> Leitura {
        self.ler_com(entrada, self.executor.as_deref())
    }

    /// Lê uma entrada com um executor emprestado (o catálogo usa em `testar`).
    pub fn ler_com(&self, entrada: &Entrada, executor: Option<&dyn ExecutorDeScript>) -> Leitura {
        let normalizado = normalizar(entrada.texto.as_deref().unwrap_or(""));
        let mut execucoes = Vec::new();
        let mut candidatas = Vec::new();
        for vc in &self.versoes {
            if !vc.versao.definicao.aplica_a.contains(&entrada.tipo) {
                continue;
            }
            let t0 = Instant::now();
            let avaliada = self.avaliar(vc, entrada, &normalizado, executor);
            let duracao_us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            let (resultado, motivo) = match avaliada {
                Ok(None) => (ResultadoDaExecucao::NaoSeAplica, None),
                Ok(Some(rec)) => {
                    candidatas.push(Candidata {
                        prioridade: vc.versao.definicao.prioridade,
                        especificidade: vc.especificidade,
                        rec,
                    });
                    (ResultadoDaExecucao::Reconhecido, None)
                }
                Err(m) => (ResultadoDaExecucao::Erro, Some(m)),
            };
            execucoes.push(Execucao {
                regra: vc.versao.regra.clone(),
                versao: vc.versao.versao,
                resultado,
                motivo,
                duracao_us,
            });
        }
        let decisao = decidir(candidatas, &mut execucoes);
        Leitura {
            entrada: entrada.id.clone(),
            decisao,
            execucoes,
        }
    }

    /// Termina um reconhecimento que pedia um leitor nativo: quem chama rodou o leitor e
    /// entrega os campos dele; aqui rodam o script, os obrigatórios e as propostas. Os
    /// campos do leitor prevalecem sobre os extraídos com o mesmo nome. Se o script falha,
    /// o reconhecimento volta com `revisar` e o motivo (a fila não trava).
    pub fn completar(
        &self,
        mut reconhecimento: Reconhecimento,
        campos_do_leitor: BTreeMap<String, Campo>,
        entrada: &Entrada,
    ) -> Reconhecimento {
        let Some(vc) = self.versoes.iter().find(|v| {
            v.versao.regra == reconhecimento.regra && v.versao.versao == reconhecimento.versao
        }) else {
            reconhecimento.revisar = true;
            reconhecimento
                .motivos
                .push("a versão da regra não está neste motor".to_owned());
            return reconhecimento;
        };
        reconhecimento.campos.extend(campos_do_leitor);
        reconhecimento.leitor = None;
        let copia = reconhecimento.clone();
        match self.finalizar(vc, entrada, reconhecimento, self.executor.as_deref()) {
            Ok(Some(r)) => r,
            Ok(None) => {
                let mut r = copia;
                r.motivos
                    .push("o script disse que a regra não corresponde".to_owned());
                r.revisar = true;
                r
            }
            Err(m) => {
                let mut r = copia;
                r.revisar = true;
                r.motivos.push(m);
                r
            }
        }
    }

    fn avaliar(
        &self,
        vc: &VersaoCompilada,
        entrada: &Entrada,
        normalizado: &str,
        executor: Option<&dyn ExecutorDeScript>,
    ) -> Result<Option<Reconhecimento>, String> {
        let comp = vc.compilada.as_ref().map_err(Clone::clone)?;
        if !comp.detecta(entrada, normalizado) {
            return Ok(None);
        }
        let (campos, motivos) = comp.extrair(entrada);
        let v = &vc.versao;
        let rec = Reconhecimento {
            regra: v.regra.clone(),
            versao: v.versao,
            checksum: v.checksum.clone(),
            tipo_documental: v.definicao.tipo_documental.clone(),
            estado: v.estado,
            campos,
            leitor: v.definicao.leitor.clone(),
            revisar: false,
            motivos,
            evidencias: Vec::new(),
            propostas: Vec::new(),
            chave: chave_de_idempotencia(&entrada.id, &v.regra, v.versao, &v.checksum, &[]),
        };
        if rec.leitor.is_some() {
            return Ok(Some(rec));
        }
        self.finalizar(vc, entrada, rec, executor)
    }

    /// Script, obrigatórios, propostas e chave. `Ok(None)`: o script disse que não
    /// corresponde.
    fn finalizar(
        &self,
        vc: &VersaoCompilada,
        entrada: &Entrada,
        mut rec: Reconhecimento,
        executor: Option<&dyn ExecutorDeScript>,
    ) -> Result<Option<Reconhecimento>, String> {
        let v = &vc.versao;
        let mut checksums_dos_parametros = Vec::new();
        if let Some(fonte) = &v.script {
            let executor = executor.ok_or("a versão tem script e não há executor")?;
            match preparar_parametros(self, vc, entrada, &rec) {
                Err(motivo) => {
                    rec.revisar = true;
                    rec.motivos.push(motivo);
                }
                Ok(usados) => {
                    let parametros: serde_json::Map<String, Json> = usados
                        .iter()
                        .map(|p| (p.nome.clone(), p.valor.clone()))
                        .collect();
                    let pedido = PedidoDeScript {
                        fonte: fonte.clone(),
                        checksum: v.checksum.clone(),
                        entrada: entrada_do_script(vc, entrada, &rec),
                        parametros: Json::Object(parametros),
                        limites: Limites::default(),
                    };
                    let resposta = executor.executar(&pedido).map_err(|e| e.to_string())?;
                    if !resposta.corresponde {
                        return Ok(None);
                    }
                    aplicar_resposta(&mut rec, resposta)?;
                    for p in &usados {
                        rec.evidencias
                            .push(format!("tabela:{}:{}", p.nome, p.versao));
                        checksums_dos_parametros.push(p.checksum.clone());
                    }
                }
            }
        }
        for nome in &v.definicao.obrigatorios {
            if !rec.campos.contains_key(nome) {
                rec.revisar = true;
                rec.motivos
                    .push(format!("campo obrigatório ausente: `{nome}`"));
            }
        }
        for modelo in &v.definicao.propor {
            match modelo.gerar(&rec.campos) {
                Ok(p) => rec.propostas.push(p),
                Err(m) => rec.motivos.push(m),
            }
        }
        rec.chave = chave_de_idempotencia(
            &entrada.id,
            &v.regra,
            v.versao,
            &v.checksum,
            &checksums_dos_parametros,
        );
        Ok(Some(rec))
    }
}

/// A competência `AAAA-MM`: o campo `competencia`, senão o mês de `data`, de `dicas.data`
/// ou de `momento`.
fn competencia_da_entrada(entrada: &Entrada, campos: &BTreeMap<String, Campo>) -> Option<String> {
    let mes = |s: &str| {
        let m = s.get(..7)?;
        crate::definicao::parse_competencia(m).filter(|c| c == m)
    };
    if let Some(Campo {
        valor: Valor::Competencia(c),
        ..
    }) = campos.get("competencia")
    {
        return Some(c.clone());
    }
    if let Some(Campo {
        valor: Valor::Data(d),
        ..
    }) = campos.get("data")
    {
        return mes(d);
    }
    entrada
        .dicas
        .data
        .as_deref()
        .and_then(mes)
        .or_else(|| entrada.momento.as_deref().and_then(mes))
}

/// Os parâmetros da definição, vigentes na competência; `Err` com o motivo se falta
/// competência ou algum parâmetro.
fn preparar_parametros<'a>(
    motor: &'a Motor,
    vc: &VersaoCompilada,
    entrada: &Entrada,
    rec: &Reconhecimento,
) -> Result<Vec<&'a Parametro>, String> {
    let nomes = &vc.versao.definicao.parametros;
    if nomes.is_empty() {
        return Ok(Vec::new());
    }
    let competencia = competencia_da_entrada(entrada, &rec.campos)
        .ok_or("sem competência para escolher os parâmetros; o script não rodou")?;
    nomes
        .iter()
        .map(|n| {
            parametro_vigente(&motor.parametros, n, &competencia).ok_or_else(|| {
                format!("sem o parâmetro `{n}` vigente em {competencia}; o script não rodou")
            })
        })
        .collect()
}

fn valor_json(v: &Valor) -> Json {
    match v {
        Valor::Texto(s) | Valor::Data(s) | Valor::Competencia(s) => json!(s),
        Valor::Centavos(n) | Valor::Inteiro(n) => json!(n),
        Valor::Booleano(b) => json!(b),
    }
}

fn entrada_do_script(vc: &VersaoCompilada, entrada: &Entrada, rec: &Reconhecimento) -> Json {
    let campos: serde_json::Map<String, Json> = rec
        .campos
        .iter()
        .map(|(n, c)| (n.clone(), valor_json(&c.valor)))
        .collect();
    let mut j = json!({
        "tipo_documental": rec.tipo_documental,
        "canal": serde_json::to_value(entrada.origem.canal).unwrap_or(Json::Null),
        "campos": campos,
    });
    if vc.versao.definicao.script_le_texto
        && let (Some(t), Some(o)) = (&entrada.texto, j.as_object_mut())
    {
        o.insert("texto".to_owned(), json!(t));
    }
    j
}

/// Converte a resposta conforme a convenção de `script.rs` e a funde no reconhecimento.
fn aplicar_resposta(rec: &mut Reconhecimento, resposta: RespostaDoScript) -> Result<(), String> {
    let a_partir_de: Vec<String> = rec.campos.keys().cloned().collect();
    if let Some(t) = resposta.tipo_documental
        && !t.trim().is_empty()
    {
        rec.tipo_documental = t;
    }
    for (nome, valor) in resposta.campos {
        let convertido = converter(&nome, &valor)
            .map_err(|m| format!("resposta do script inválida: campo `{nome}` {m}"))?;
        rec.campos.insert(
            nome,
            Campo {
                valor: convertido,
                de_onde: DeOnde::Script {
                    a_partir_de: a_partir_de.clone(),
                },
            },
        );
    }
    rec.revisar |= resposta.revisar;
    rec.motivos.extend(resposta.motivos);
    rec.evidencias.extend(resposta.evidencias);
    Ok(())
}

fn converter(nome: &str, valor: &Json) -> Result<Valor, String> {
    match valor {
        Json::Number(n) => {
            let inteiro = n.as_i64().ok_or("não é um inteiro")?;
            if nome.ends_with("_centavos") {
                Ok(Valor::Centavos(inteiro))
            } else {
                Ok(Valor::Inteiro(inteiro))
            }
        }
        Json::String(s) => {
            if nome.ends_with("_centavos") {
                return Err("em centavos precisa ser um inteiro".to_owned());
            }
            if (nome.starts_with("data") || nome.starts_with("vencimento"))
                && let Some(d) = crate::definicao::parse_data(s)
                && d == *s
            {
                return Ok(Valor::Data(d));
            }
            Ok(Valor::Texto(s.clone()))
        }
        Json::Bool(b) if !nome.ends_with("_centavos") => Ok(Valor::Booleano(*b)),
        _ => Err("tem um tipo que não é aceito (use texto, inteiro ou booleano)".to_owned()),
    }
}

// ---------------------------------------------------------------------------------------
// Precedência
// ---------------------------------------------------------------------------------------

fn decidir(mut candidatas: Vec<Candidata>, execucoes: &mut [Execucao]) -> Decisao {
    if candidatas.is_empty() {
        return Decisao::Nenhuma;
    }
    candidatas.sort_by(|a, b| {
        (b.prioridade, b.especificidade)
            .cmp(&(a.prioridade, a.especificidade))
            .then_with(|| (&a.rec.regra, a.rec.versao).cmp(&(&b.rec.regra, b.rec.versao)))
    });
    let (p0, e0) = (candidatas[0].prioridade, candidatas[0].especificidade);
    let empatadas = candidatas
        .iter()
        .take_while(|c| (c.prioridade, c.especificidade) == (p0, e0))
        .count();
    let nota = |execucoes: &mut [Execucao], regra: &str, versao: u32, motivo: String| {
        if let Some(e) = execucoes
            .iter_mut()
            .find(|e| e.regra == regra && e.versao == versao)
        {
            e.motivo = Some(motivo);
        }
    };
    if empatadas == 1 {
        let segunda = candidatas.get(1).map(|s| {
            (
                s.prioridade,
                s.especificidade,
                s.rec.regra.clone(),
                s.rec.versao,
            )
        });
        let mut vencedora = candidatas.remove(0).rec;
        if let Some((p1, e1, regra1, _)) = &segunda {
            let porque = if p0 > *p1 {
                format!("prioridade {p0} > {p1} da regra {regra1}")
            } else {
                format!("mais específica: {e0} condições contra {e1} (regra {regra1})")
            };
            vencedora.motivos.push(porque);
        }
        for c in &candidatas {
            nota(
                execucoes,
                &c.rec.regra,
                c.rec.versao,
                format!("não prevaleceu: venceu a regra {}", vencedora.regra),
            );
        }
        return Decisao::Reconhecido(Box::new(vencedora));
    }
    candidatas.truncate(empatadas);
    if let Some(discordancia) = discordam(&candidatas) {
        let nomes: Vec<String> = candidatas
            .iter()
            .map(|c| format!("{}@{}", c.rec.regra, c.rec.versao))
            .collect();
        let motivo = format!(
            "regras empatadas em prioridade {p0} e especificidade {e0} discordam em {discordancia}: {}",
            nomes.join(", ")
        );
        for c in &candidatas {
            nota(execucoes, &c.rec.regra, c.rec.versao, motivo.clone());
        }
        return Decisao::Ambiguo {
            candidatas: candidatas.into_iter().map(|c| c.rec).collect(),
            motivo,
        };
    }
    let mut resto = candidatas.into_iter();
    let Some(primeira) = resto.next() else {
        return Decisao::Nenhuma;
    };
    let mut vencedora = primeira.rec;
    vencedora.motivos.push(format!(
        "empate de prioridade {p0} e especificidade {e0} com resultados concordantes; \
         escolhida a de menor id ({})",
        vencedora.regra
    ));
    for c in resto {
        nota(
            execucoes,
            &c.rec.regra,
            c.rec.versao,
            format!("empate concordante: venceu a regra {}", vencedora.regra),
        );
    }
    Decisao::Reconhecido(Box::new(vencedora))
}

/// Em que as candidatas discordam (tipo ou campo crítico com valores diferentes nas duas),
/// ou `None` se concordam.
fn discordam(candidatas: &[Candidata]) -> Option<String> {
    for (i, a) in candidatas.iter().enumerate() {
        for b in &candidatas[i + 1..] {
            if a.rec.tipo_documental != b.rec.tipo_documental {
                return Some("tipo_documental".to_owned());
            }
            for nome in CAMPOS_CRITICOS {
                if let (Some(x), Some(y)) = (a.rec.campos.get(nome), b.rec.campos.get(nome))
                    && x.valor != y.valor
                {
                    return Some(format!("`{nome}`"));
                }
            }
        }
    }
    None
}
