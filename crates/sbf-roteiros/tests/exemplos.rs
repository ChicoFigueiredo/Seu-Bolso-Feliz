//! Os exemplos de `exemplos/`: os manifestos passam pela validação do importar, e os `.luau`
//! rodam no host Luau contra verbos de mentira (só os verbos do contrato existem, a sintaxe
//! compila e o fluxo faz o que o comentário promete). Nada de rede nem de Chrome.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_navegador::luau::{Limites, ResultadoDeVerbo, Saida, Verbos, executar};
use sbf_roteiros::manifesto::{Classe, Manifesto, Mfa};
use std::time::Duration;

const CONTRACHEQUE_JSON: &str = include_str!("../exemplos/contracheque-exemplo.json");
const CONTRACHEQUE_LUAU: &str = include_str!("../exemplos/contracheque-exemplo.luau");
const CONEXOES_JSON: &str = include_str!("../exemplos/atualizar-conexoes-exemplo.json");
const CONEXOES_LUAU: &str = include_str!("../exemplos/atualizar-conexoes-exemplo.luau");
const PLUGGY_JSON: &str = include_str!("../exemplos/meu-pluggy-atualizar.json");
const PLUGGY_LUAU: &str = include_str!("../exemplos/meu-pluggy-atualizar.luau");

/// Os únicos nomes que um roteiro pode usar (a tabela do contrato).
const VERBOS_DO_CONTRATO: &[&str] = &[
    "abrir",
    "esperar",
    "existe",
    "esperar_sumir",
    "clicar",
    "texto",
    "tabela",
    "preencher",
    "preencher_senha",
    "preencher_cpf",
    "preencher_codigo_do_email",
    "baixar",
    "pausa",
    "registrar",
    "hoje",
    "somar_meses",
];

/// Os verbos de mentira: anotam cada chamada e respondem "visível" conforme o teste manda.
struct Fake {
    visivel: Box<dyn Fn(&str) -> bool>,
    hoje: &'static str,
    chamadas: Vec<String>,
}

impl Fake {
    fn novo(hoje: &'static str, visivel: impl Fn(&str) -> bool + 'static) -> Self {
        Self {
            visivel: Box::new(visivel),
            hoje,
            chamadas: Vec::new(),
        }
    }

    fn anotar(&mut self, c: String) {
        self.chamadas.push(c);
    }

    fn com_prefixo(&self, prefixo: &str) -> Vec<&str> {
        self.chamadas
            .iter()
            .filter(|c| c.starts_with(prefixo))
            .map(String::as_str)
            .collect()
    }
}

impl Verbos for Fake {
    fn abrir(&mut self, url: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("abrir {url}"));
        Ok(())
    }
    fn esperar(&mut self, seletor: &str, _prazo: Duration) -> ResultadoDeVerbo<bool> {
        self.anotar(format!("esperar {seletor}"));
        Ok((self.visivel)(seletor))
    }
    fn existe(&mut self, seletor: &str) -> ResultadoDeVerbo<bool> {
        self.anotar(format!("existe {seletor}"));
        Ok((self.visivel)(seletor))
    }
    fn clicar(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("clicar {seletor}"));
        Ok(())
    }
    fn esperar_sumir(&mut self, seletor: &str, _prazo: Duration) -> ResultadoDeVerbo<bool> {
        self.anotar(format!("esperar_sumir {seletor}"));
        Ok(true)
    }
    fn texto(&mut self, seletor: &str) -> ResultadoDeVerbo<String> {
        // Só o painel do Meu Pluggy lê texto: o botão de ação é "Atualizar", o item é um banco.
        Ok(if seletor.contains("button:nth-of-type(1)") {
            "Atualizar".to_owned()
        } else if seletor.contains("a:nth-of-type(") {
            "Banco Um Hoje Ver detalhes".to_owned()
        } else {
            String::new()
        })
    }
    fn tabela(&mut self, _s: &str) -> ResultadoDeVerbo<Vec<Vec<String>>> {
        Ok(Vec::new())
    }
    fn preencher(&mut self, seletor: &str, _texto: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("preencher {seletor}"));
        Ok(())
    }
    fn preencher_senha(&mut self, seletor: &str, rotulo: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("preencher_senha {seletor} {rotulo}"));
        Ok(())
    }
    fn preencher_cpf(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("preencher_cpf {seletor}"));
        Ok(())
    }
    fn preencher_codigo_do_email(
        &mut self,
        seletor: &str,
        janela: Duration,
    ) -> ResultadoDeVerbo<()> {
        self.anotar(format!(
            "preencher_codigo_do_email {seletor} {}",
            janela.as_secs()
        ));
        Ok(())
    }
    fn baixar(&mut self, seletor: &str, nome: Option<&str>) -> ResultadoDeVerbo<String> {
        self.anotar(format!("baixar {seletor}"));
        Ok(nome.unwrap_or("arquivo.pdf").to_owned())
    }
    fn pausa(&mut self, motivo: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("pausa {motivo}"));
        Ok(())
    }
    fn registrar(&mut self, mensagem: &str) -> ResultadoDeVerbo<()> {
        self.anotar(format!("registrar {mensagem}"));
        Ok(())
    }
    fn hoje(&mut self) -> ResultadoDeVerbo<String> {
        Ok(self.hoje.to_owned())
    }
    fn somar_meses(&mut self, data: &str, meses: i32) -> ResultadoDeVerbo<String> {
        // Só o que os exemplos usam: recuar de mês.
        let ano: i32 = data[..4].parse().unwrap();
        let mes: i32 = data[5..7].parse().unwrap();
        let total = ano * 12 + (mes - 1) + meses;
        Ok(format!(
            "{:04}-{:02}-{}",
            total.div_euclid(12),
            total.rem_euclid(12) + 1,
            &data[8..]
        ))
    }
}

fn rodar(fonte: &str, fake: &mut Fake) -> Result<Saida, bancada_navegador::luau::ErroDeRoteiro> {
    executar(fonte, fake, &Limites::default())
}

// ----- manifestos --------------------------------------------------------------------------

#[test]
fn manifesto_do_contracheque_valida() {
    let m = Manifesto::de_json(CONTRACHEQUE_JSON).unwrap();
    assert_eq!(m.nome, "contracheque-exemplo");
    assert_eq!(m.hosts, ["portal.exemplo.test"]);
    assert_eq!(m.url_inicial, "https://portal.exemplo.test/entrar");
    assert_eq!(m.rotulo_senha.as_deref(), Some("senha:portal-exemplo"));
    assert_eq!(
        m.mfa,
        Mfa::Email {
            remetente: "nao-responda@portal.exemplo.test".into(),
            formato: "\\d{6}".into(),
        }
    );
    assert!(!m.movimenta_dinheiro);
    assert_eq!(m.classe, Classe::Consulta);
    assert_eq!(m.agenda_horas, Some(168));
}

#[test]
fn manifesto_de_atualizar_conexoes_valida() {
    let m = Manifesto::de_json(CONEXOES_JSON).unwrap();
    assert_eq!(m.nome, "atualizar-conexoes-exemplo");
    assert_eq!(m.classe, Classe::AtualizarConexao);
    assert_eq!(m.mfa, Mfa::Manual);
    assert!(!m.movimenta_dinheiro);
    assert_eq!(m.hosts, ["portal-de-conexoes.exemplo.test"]);
}

#[test]
fn manifesto_do_meu_pluggy_valida() {
    let m = Manifesto::de_json(PLUGGY_JSON).unwrap();
    assert_eq!(m.nome, "meu-pluggy-atualizar");
    assert_eq!(m.classe, Classe::AtualizarConexao);
    assert_eq!(m.mfa, Mfa::Manual);
    assert!(!m.movimenta_dinheiro);
    assert!(m.rotulo_senha.is_none(), "o login é por e-mail: sem senha");
    assert!(m.hosts.contains(&"meu.pluggy.ai".to_owned()));
    assert_eq!(m.url_inicial, "https://meu.pluggy.ai/connections");
    assert_eq!(m.agenda_horas, Some(24));
}

// ----- fontes ------------------------------------------------------------------------------

/// Os nomes depois de `navegador.` e `util.` no fonte.
fn verbos_usados(fonte: &str) -> Vec<String> {
    let mut achados = Vec::new();
    for tabela in ["navegador.", "util."] {
        let mut resto = fonte;
        while let Some(i) = resto.find(tabela) {
            // Ignora o que vem colado a outro nome (`xnavegador.`).
            let antes = resto[..i].chars().next_back();
            resto = &resto[i + tabela.len()..];
            if antes.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let nome: String = resto
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            achados.push(nome);
        }
    }
    achados
}

fn sem_comentarios(fonte: &str) -> String {
    fonte
        .lines()
        .map(|l| l.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn os_roteiros_usam_so_verbos_do_contrato_e_nada_proibido() {
    for (nome, fonte) in [
        ("contracheque", CONTRACHEQUE_LUAU),
        ("conexoes", CONEXOES_LUAU),
        ("meu-pluggy", PLUGGY_LUAU),
    ] {
        assert!(!fonte.trim().is_empty(), "{nome}: vazio");
        assert!(fonte.len() <= 64 * 1024, "{nome}: passa de 64 KiB");
        let codigo = sem_comentarios(fonte);
        let usados = verbos_usados(&codigo);
        assert!(!usados.is_empty(), "{nome}: não chama verbo nenhum");
        for v in &usados {
            assert!(
                VERBOS_DO_CONTRATO.contains(&v.as_str()),
                "{nome}: verbo fora do contrato: {v}"
            );
        }
        for proibido in [
            "require",
            "loadstring",
            "debug",
            "getfenv",
            "setfenv",
            "os.",
            "io.",
        ] {
            assert!(
                !codigo.contains(proibido),
                "{nome}: usa `{proibido}`, que o sandbox não oferece"
            );
        }
    }
}

// ----- contracheque ------------------------------------------------------------------------

fn portal_com(meses: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |s| match s {
        "#cpf" | "#codigo" | "#lista" => true,
        outro => meses.iter().any(|m| outro == format!("#baixar-{m}")),
    }
}

#[test]
fn contracheque_baixa_o_mes_corrente() {
    let manifesto = Manifesto::de_json(CONTRACHEQUE_JSON).unwrap();
    let mut fake = Fake::novo("2026-11-24", portal_com(&["2026-10", "2026-11"]));
    let saida = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap();
    assert_eq!(saida.arquivos, ["contracheque-2026-11.pdf"]);
    assert!(saida.registros.iter().any(|r| r.contains("2026-11")));

    // Abre a URL do manifesto e usa o rótulo do manifesto.
    assert_eq!(
        fake.com_prefixo("abrir "),
        [format!("abrir {}", manifesto.url_inicial)]
    );
    assert_eq!(
        fake.com_prefixo("preencher_senha"),
        [format!(
            "preencher_senha #senha {}",
            manifesto.rotulo_senha.unwrap()
        )]
    );
    assert_eq!(fake.com_prefixo("preencher_cpf"), ["preencher_cpf #cpf"]);
    assert_eq!(
        fake.com_prefixo("preencher_codigo_do_email"),
        ["preencher_codigo_do_email #codigo 120"]
    );
    assert_eq!(fake.com_prefixo("baixar"), ["baixar #baixar-2026-11"]);
    // A ordem do fluxo: entrar, código, confirmar, só então baixar.
    let pos = |c: &str| fake.chamadas.iter().position(|x| x == c).unwrap();
    assert!(pos("clicar #entrar") < pos("preencher_codigo_do_email #codigo 120"));
    assert!(pos("preencher_codigo_do_email #codigo 120") < pos("clicar #confirmar"));
    assert!(pos("clicar #confirmar") < pos("baixar #baixar-2026-11"));
}

#[test]
fn contracheque_so_clica_no_que_precisa() {
    let mut fake = Fake::novo("2026-11-24", portal_com(&["2026-11"]));
    rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap();
    assert_eq!(
        fake.com_prefixo("clicar"),
        ["clicar #entrar", "clicar #confirmar"]
    );
    for c in &fake.chamadas {
        let c = c.to_lowercase();
        assert!(!c.contains("pagar") && !c.contains("alterar"), "{c}");
    }
}

#[test]
fn contracheque_cai_para_o_mes_anterior_se_o_corrente_nao_saiu() {
    let mut fake = Fake::novo("2026-11-24", portal_com(&["2026-10"]));
    let saida = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap();
    assert_eq!(saida.arquivos, ["contracheque-2026-10.pdf"]);
    assert_eq!(fake.com_prefixo("baixar"), ["baixar #baixar-2026-10"]);
}

#[test]
fn contracheque_vira_o_ano_ao_recuar_de_janeiro() {
    let mut fake = Fake::novo("2027-01-05", portal_com(&["2026-12"]));
    let saida = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap();
    assert_eq!(saida.arquivos, ["contracheque-2026-12.pdf"]);
}

#[test]
fn contracheque_mes_nao_publicado_registra_e_sai_sem_erro() {
    let mut fake = Fake::novo("2026-11-24", portal_com(&["2026-09"]));
    let saida = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap();
    assert!(saida.arquivos.is_empty());
    assert_eq!(saida.registros.len(), 1);
    assert!(saida.registros[0].contains("ainda não publicado"));
    assert!(fake.com_prefixo("baixar").is_empty());
}

#[test]
fn contracheque_para_com_erro_claro_se_a_lista_nao_aparece() {
    let mut fake = Fake::novo("2026-11-24", |s| matches!(s, "#cpf" | "#codigo"));
    let erro = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap_err().to_string();
    assert!(erro.contains("lista de contracheques"), "{erro}");
}

#[test]
fn contracheque_avisa_se_o_portal_recusa_a_entrada() {
    let mut fake = Fake::novo("2026-11-24", |s| matches!(s, "#cpf" | "#erro"));
    let erro = rodar(CONTRACHEQUE_LUAU, &mut fake).unwrap_err().to_string();
    assert!(erro.contains("recusou a entrada"), "{erro}");
    assert!(fake.com_prefixo("preencher_codigo_do_email").is_empty());
}

// ----- atualizar conexões ------------------------------------------------------------------

fn conexoes(total: usize, com_consentimento: bool) -> impl Fn(&str) -> bool {
    move |s| {
        if s == "#conexoes" {
            return true;
        }
        if s == "#consentimento" {
            return com_consentimento;
        }
        for i in 1..=total {
            if s == format!(".conexao:nth-child({i}) .atualizar") {
                return true;
            }
        }
        false
    }
}

#[test]
fn atualizar_conexoes_clica_em_cada_uma_e_pausa_no_consentimento() {
    let mut fake = Fake::novo("2026-11-24", conexoes(3, true));
    let saida = rodar(CONEXOES_LUAU, &mut fake).unwrap();
    assert_eq!(
        fake.com_prefixo("clicar"),
        [
            "clicar .conexao:nth-child(1) .atualizar",
            "clicar .conexao:nth-child(2) .atualizar",
            "clicar .conexao:nth-child(3) .atualizar",
        ]
    );
    assert_eq!(fake.com_prefixo("pausa").len(), 3);
    assert!(
        fake.com_prefixo("pausa")
            .iter()
            .all(|p| p.contains("consentimento"))
    );
    assert_eq!(saida.registros, ["conexões atualizadas: 3"]);
}

#[test]
fn atualizar_conexoes_sem_consentimento_nao_pausa() {
    let mut fake = Fake::novo("2026-11-24", conexoes(2, false));
    rodar(CONEXOES_LUAU, &mut fake).unwrap();
    assert!(fake.com_prefixo("pausa").is_empty());
    assert_eq!(fake.com_prefixo("clicar").len(), 2);
}

#[test]
fn atualizar_conexoes_falha_se_o_painel_nao_aparece() {
    let mut fake = Fake::novo("2026-11-24", |_| false);
    let erro = rodar(CONEXOES_LUAU, &mut fake).unwrap_err().to_string();
    assert!(erro.contains("painel de conexões"), "{erro}");
}

// ----- Meu Pluggy --------------------------------------------------------------------------

/// Três conexões na lista; o widget abre depois de cada "Atualizar" e some.
fn painel_do_pluggy(com_widget: bool) -> impl Fn(&str) -> bool {
    move |s| {
        if s.contains("zoid-pluggy-connect-widget") {
            return com_widget;
        }
        if let Some(i) = s.split("a:nth-of-type(").nth(1) {
            let n: u32 = i.trim_end_matches(')').parse().unwrap_or(99);
            return n <= 3;
        }
        true
    }
}

#[test]
fn meu_pluggy_atualiza_cada_conexao_e_so_clica_em_atualizar() {
    let mut fake = Fake::novo("2026-11-25", painel_do_pluggy(true));
    let saida = rodar(PLUGGY_LUAU, &mut fake).unwrap();
    assert!(
        saida
            .registros
            .iter()
            .any(|r| r.contains("conexões atualizadas: 3 de 3")),
        "{:?}",
        saida.registros
    );
    let cliques = fake.com_prefixo("clicar ");
    let atualizar: Vec<_> = cliques
        .iter()
        .filter(|c| c.ends_with("div:nth-of-type(2) > button:nth-of-type(1)"))
        .collect();
    assert_eq!(atualizar.len(), 3, "{cliques:?}");
    // Nunca o segundo botão do bloco ("Excluir").
    assert!(
        !cliques.iter().any(|c| c.contains("button:nth-of-type(2)")),
        "{cliques:?}"
    );
    assert_eq!(fake.com_prefixo("esperar_sumir ").len(), 3);
    assert!(fake.com_prefixo("preencher").is_empty(), "sem segredo");
}

#[test]
fn meu_pluggy_falha_quando_o_widget_nao_abre() {
    let mut fake = Fake::novo("2026-11-25", painel_do_pluggy(false));
    let erro = rodar(PLUGGY_LUAU, &mut fake).unwrap_err();
    assert!(erro.to_string().contains("não atualizaram"), "{erro}");
}
