//! [`Chaveiro`]: o chaveiro do Seu Bolso Feliz sobre a [`GuardaDeSegredos`] da bancada.

use crate::arquivo_antigo::{self, SegredosAntigos};
use crate::cpf::cpf_valido;
use crate::erro::ErroDoChaveiro;
use crate::pasta::{garantir_fora_do_cofre, pasta_padrao_do_chaveiro};
use bancada_segredos::{
    ChaveiroDoSistema, ConfiguracaoDaGuarda, ErroDeSegredo, GuardaDeSegredos,
    ParametrosDeDerivacao, Segredo, Situacao, TAMANHO_MAXIMO_DO_ROTULO,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// O serviço sob o qual os segredos ficam no keyring do sistema.
pub const SERVICO: &str = "Seu Bolso Feliz";
/// A chave do CPF no chaveiro.
pub const CHAVE_DO_CPF: &str = "cpf";
/// O prefixo das senhas próprias: a chave de `<rotulo>` é `senha:<rotulo>`.
pub const PREFIXO_DE_SENHA: &str = "senha:";
/// O prefixo da credencial de estação (spec 07, "Autenticação da estação"): a chave do
/// servidor `<servidor>` é `estacao:<servidor>`.
pub const PREFIXO_DE_ESTACAO: &str = "estacao:";
/// O prefixo das chaves de provedores de IA (spec 09): a do OpenRouter é `ia:openrouter`.
pub const PREFIXO_DE_IA: &str = "ia:";
/// O prefixo das senhas de sites dos roteiros (ADR-016): a chave do rótulo `<rotulo>` é
/// `site:<rotulo>`. Fica separado de `senha:` (PDFs) para as duas listas não se misturarem.
pub const PREFIXO_DE_SITE: &str = "site:";
/// Prefixo das chaves de sessão dos mensageiros (a chave que cifra a sessão do WhatsApp).
pub const PREFIXO_DE_MENSAGEIRO: &str = "mensageiro:";
/// Prefixo das chaves das conexões de e-mail (refresh token, client id e secret do aplicativo):
/// `conexao:email/<id>/refresh_token`. Fica separado de `mensageiro:` para o contrato de cada
/// namespace valer só para o seu crate.
pub const PREFIXO_DE_CONEXAO: &str = "conexao:";

/// O que a importação trouxe, sem nenhum valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDaImportacao {
    /// O CPF importado confere com os dígitos verificadores.
    pub cpf_valido: bool,
    /// Os rótulos das senhas próprias importadas, em ordem.
    pub rotulos: Vec<String>,
    /// Onde os segredos ficaram: keyring do sistema ou arquivo de fallback.
    pub onde: Situacao,
}

/// A oferta de apagar o `cofre.enc` antigo. Só nasce de uma conferência bem-sucedida
/// ([`Chaveiro::conferir_importacao`]) e só apaga quando o usuário aceita
/// ([`OfertaDeApagar::apagar`]): o chaveiro nunca apaga o arquivo sozinho.
#[derive(Debug)]
#[must_use = "a oferta só apaga se o usuário aceitar; descartar é recusar"]
pub struct OfertaDeApagar {
    caminho: PathBuf,
}

impl OfertaDeApagar {
    /// O `cofre.enc` que seria apagado.
    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// O usuário aceitou: apaga o `cofre.enc`. (Ele é cifrado; não há sobrescrita antes,
    /// porque em SSD e em NTFS ela não garante nada.)
    pub fn apagar(self) -> Result<(), ErroDoChaveiro> {
        std::fs::remove_file(&self.caminho).map_err(|e| ErroDoChaveiro::disco(&e))
    }
}

/// O chaveiro do SBF: o CPF (chave `cpf`) e as senhas próprias de PDF (chaves
/// `senha:<rotulo>`), no keyring do sistema ou, sem ele, no arquivo de fallback cifrado por
/// senha-mestra na pasta de dados do app. Barato de clonar. O `Debug` não mostra valores.
#[derive(Debug, Clone)]
pub struct Chaveiro {
    guarda: GuardaDeSegredos,
    pasta: PathBuf,
}

fn rotulo_de_senha_valido(rotulo: &str) -> bool {
    !rotulo.is_empty()
        && PREFIXO_DE_SENHA.len() + rotulo.len() <= TAMANHO_MAXIMO_DO_ROTULO
        && !rotulo.chars().any(char::is_control)
}

fn chave_de_senha(rotulo: &str) -> Result<String, ErroDoChaveiro> {
    if rotulo_de_senha_valido(rotulo) {
        Ok(format!("{PREFIXO_DE_SENHA}{rotulo}"))
    } else {
        Err(ErroDoChaveiro::RotuloInvalido)
    }
}

/// Os dois segredos são iguais, em tempo constante.
fn iguais(a: &Segredo, b: &Segredo) -> bool {
    bool::from(a.expor().as_bytes().ct_eq(b.expor().as_bytes()))
}

impl Chaveiro {
    /// O chaveiro na pasta padrão (`<pasta de dados do app>/chaveiro`), com o keyring do
    /// sistema.
    pub fn abrir_na_pasta_padrao() -> Result<Self, ErroDoChaveiro> {
        Self::abrir(&pasta_padrao_do_chaveiro()?)
    }

    /// O chaveiro numa pasta, com o keyring do sistema.
    pub fn abrir(pasta: &Path) -> Result<Self, ErroDoChaveiro> {
        Self::abrir_com(ConfiguracaoDaGuarda::nova(SERVICO, pasta), pasta)
    }

    /// O chaveiro numa pasta com outro keyring e outro custo do Argon2id (os testes usam o
    /// keyring falso e o custo baixo da bancada).
    pub fn abrir_com_chaveiro(
        pasta: &Path,
        chaveiro: Arc<dyn ChaveiroDoSistema>,
        derivacao: ParametrosDeDerivacao,
    ) -> Result<Self, ErroDoChaveiro> {
        Self::abrir_com(
            ConfiguracaoDaGuarda::nova(SERVICO, pasta)
                .chaveiro(chaveiro)
                .derivacao(derivacao),
            pasta,
        )
    }

    fn abrir_com(configuracao: ConfiguracaoDaGuarda, pasta: &Path) -> Result<Self, ErroDoChaveiro> {
        Ok(Self {
            guarda: GuardaDeSegredos::abrir(configuracao)?,
            pasta: pasta.to_path_buf(),
        })
    }

    /// A pasta do chaveiro (índice de rótulos e, sem keyring, o arquivo de fallback).
    pub fn pasta(&self) -> &Path {
        &self.pasta
    }

    // ---------------- Situação (para os bastidores) ----------------

    /// Keyring do sistema, arquivo trancado, arquivo aberto ou indisponível.
    pub fn situacao(&self) -> Situacao {
        self.guarda.situacao()
    }

    /// Há CPF guardado.
    pub fn tem_cpf(&self) -> bool {
        self.guarda.rotulos().iter().any(|r| r == CHAVE_DO_CPF)
    }

    /// Os rótulos das senhas próprias, sem o prefixo e sem valores, em ordem. É tudo o que
    /// os bastidores mostram.
    pub fn rotulos_de_senha(&self) -> Vec<String> {
        self.guarda
            .rotulos()
            .into_iter()
            .filter_map(|r| r.strip_prefix(PREFIXO_DE_SENHA).map(str::to_owned))
            .collect()
    }

    // ---------------- Senhas de sites (roteiros, ADR-016) ----------------

    fn chave_de_site(rotulo: &str) -> Result<String, ErroDoChaveiro> {
        if rotulo.is_empty()
            || PREFIXO_DE_SITE.len() + rotulo.len() > TAMANHO_MAXIMO_DO_ROTULO
            || rotulo.chars().any(char::is_control)
        {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        Ok(format!("{PREFIXO_DE_SITE}{rotulo}"))
    }

    /// Guarda (ou troca) a senha do site de rótulo `rotulo`.
    pub fn guardar_senha_de_site(
        &self,
        rotulo: &str,
        senha: Segredo,
    ) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.gravar(&Self::chave_de_site(rotulo)?, senha)?)
    }

    /// A senha do site, em claro. Só o executor de roteiros chama isto, e só entre o chaveiro
    /// e o campo do navegador.
    pub fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&Self::chave_de_site(rotulo)?)?)
    }

    /// Os rótulos das senhas de sites, sem prefixo e sem valores, em ordem.
    pub fn rotulos_de_site(&self) -> Vec<String> {
        self.guarda
            .rotulos()
            .into_iter()
            .filter_map(|r| r.strip_prefix(PREFIXO_DE_SITE).map(str::to_owned))
            .collect()
    }

    /// Apaga a senha do site (sem erro se já não havia).
    pub fn apagar_senha_de_site(&self, rotulo: &str) -> Result<(), ErroDoChaveiro> {
        match self.guarda.apagar(&Self::chave_de_site(rotulo)?) {
            Ok(()) | Err(bancada_segredos::ErroDeSegredo::NaoEncontrado) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // ---------------- Arquivo de fallback ----------------

    /// Sem keyring do sistema e sem arquivo ainda: cria o arquivo de fallback com a
    /// senha-mestra (fica aberto).
    pub fn criar_arquivo(&self, senha_mestra: &Segredo) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.criar_arquivo(senha_mestra)?)
    }

    /// Destranca o arquivo de fallback (uma vez por sessão). Senha errada e arquivo
    /// adulterado dão o mesmo erro.
    pub fn destrancar(&self, senha_mestra: &Segredo) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.destrancar(senha_mestra)?)
    }

    /// Tranca o arquivo de fallback: chave e valores saem da memória.
    pub fn trancar(&self) {
        self.guarda.trancar();
    }

    // ---------------- Segredos ----------------

    /// Guarda (ou troca) o CPF.
    pub fn guardar_cpf(&self, cpf: Segredo) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.gravar(CHAVE_DO_CPF, cpf)?)
    }

    /// Guarda o CPF digitado à mão (na janela): confere os dígitos verificadores e grava só os 11
    /// dígitos, sem máscara. O texto digitado é de quem chama (a janela o zera).
    ///
    /// # Errors
    /// [`ErroDoChaveiro::CpfInvalido`] (sem repetir o número) ou falha da guarda.
    pub fn guardar_cpf_digitado(&self, digitado: &str) -> Result<(), ErroDoChaveiro> {
        if !crate::cpf::cpf_valido(digitado) {
            return Err(ErroDoChaveiro::CpfInvalido);
        }
        let digitos: String = digitado.chars().filter(char::is_ascii_digit).collect();
        self.guardar_cpf(Segredo::novo(digitos))
    }

    /// Guarda (ou troca) a senha própria do rótulo.
    pub fn guardar_senha(&self, rotulo: &str, senha: Segredo) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.gravar(&chave_de_senha(rotulo)?, senha)?)
    }

    /// Apaga a senha própria do rótulo.
    pub fn apagar_senha(&self, rotulo: &str) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.apagar(&chave_de_senha(rotulo)?)?)
    }

    /// O CPF em claro. Só a abertura de PDF protegido chama isto.
    pub fn cpf(&self) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(CHAVE_DO_CPF)?)
    }

    /// A senha própria do rótulo, em claro. Só a abertura de PDF protegido chama isto.
    pub fn senha(&self, rotulo: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&chave_de_senha(rotulo)?)?)
    }

    // ---------------- Chave de provedor de IA ----------------

    fn chave_de_ia_de(provedor: &str) -> Result<String, ErroDoChaveiro> {
        if provedor.is_empty()
            || PREFIXO_DE_IA.len() + provedor.len() > TAMANHO_MAXIMO_DO_ROTULO
            || provedor.chars().any(char::is_control)
        {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        Ok(format!("{PREFIXO_DE_IA}{provedor}"))
    }

    /// Guarda (ou troca) a chave do provedor de IA `provedor` (`openrouter`).
    pub fn guardar_chave_de_ia(
        &self,
        provedor: &str,
        chave: Segredo,
    ) -> Result<(), ErroDoChaveiro> {
        Ok(self
            .guarda
            .gravar(&Self::chave_de_ia_de(provedor)?, chave)?)
    }

    /// A chave do provedor de IA, em claro. Só o portão de rede do provedor chama isto.
    pub fn chave_de_ia(&self, provedor: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&Self::chave_de_ia_de(provedor)?)?)
    }

    /// Há chave guardada para o provedor (sem ler o valor).
    pub fn tem_chave_de_ia(&self, provedor: &str) -> bool {
        Self::chave_de_ia_de(provedor).is_ok_and(|c| self.guarda.rotulos().contains(&c))
    }

    /// Apaga a chave do provedor de IA (sem erro se já não havia).
    pub fn apagar_chave_de_ia(&self, provedor: &str) -> Result<(), ErroDoChaveiro> {
        Ok(self.guarda.apagar(&Self::chave_de_ia_de(provedor)?)?)
    }

    // ---------------- Chave de sessão de mensageiro ----------------

    fn chave_de_mensageiro(nome: &str) -> Result<String, ErroDoChaveiro> {
        if nome.is_empty()
            || PREFIXO_DE_MENSAGEIRO.len() + nome.len() > TAMANHO_MAXIMO_DO_ROTULO
            || nome.chars().any(char::is_control)
        {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        Ok(format!("{PREFIXO_DE_MENSAGEIRO}{nome}"))
    }

    /// Guarda (ou troca) o segredo de sessão do mensageiro `nome` (`whatsapp`).
    pub fn guardar_segredo_de_mensageiro(
        &self,
        nome: &str,
        segredo: Segredo,
    ) -> Result<(), ErroDoChaveiro> {
        Ok(self
            .guarda
            .gravar(&Self::chave_de_mensageiro(nome)?, segredo)?)
    }

    /// O segredo de sessão do mensageiro, em claro. Só o crate dos mensageiros chama isto.
    pub fn segredo_de_mensageiro(&self, nome: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&Self::chave_de_mensageiro(nome)?)?)
    }

    /// Há segredo guardado para o mensageiro (sem ler o valor).
    pub fn tem_segredo_de_mensageiro(&self, nome: &str) -> bool {
        Self::chave_de_mensageiro(nome).is_ok_and(|c| self.guarda.rotulos().contains(&c))
    }

    /// Apaga o segredo do mensageiro (sem erro se já não havia).
    pub fn apagar_segredo_de_mensageiro(&self, nome: &str) -> Result<(), ErroDoChaveiro> {
        match self.guarda.apagar(&Self::chave_de_mensageiro(nome)?) {
            Ok(()) | Err(bancada_segredos::ErroDeSegredo::NaoEncontrado) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // ---------------- Segredos de conexões (e-mail) ----------------

    fn chave_de_conexao(nome: &str) -> Result<String, ErroDoChaveiro> {
        if nome.is_empty()
            || PREFIXO_DE_CONEXAO.len() + nome.len() > TAMANHO_MAXIMO_DO_ROTULO
            || nome.chars().any(char::is_control)
        {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        Ok(format!("{PREFIXO_DE_CONEXAO}{nome}"))
    }

    /// Guarda (ou troca) um segredo de conexão (`email/gmail/refresh_token`). Só o crate do
    /// e-mail chama isto.
    pub fn guardar_segredo_de_conexao(
        &self,
        nome: &str,
        segredo: Segredo,
    ) -> Result<(), ErroDoChaveiro> {
        Ok(self
            .guarda
            .gravar(&Self::chave_de_conexao(nome)?, segredo)?)
    }

    /// O segredo da conexão, em claro. Só o crate do e-mail chama isto.
    pub fn segredo_de_conexao(&self, nome: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&Self::chave_de_conexao(nome)?)?)
    }

    /// Há segredo guardado para a conexão (sem ler o valor).
    pub fn tem_segredo_de_conexao(&self, nome: &str) -> bool {
        Self::chave_de_conexao(nome).is_ok_and(|c| self.guarda.rotulos().contains(&c))
    }

    /// Apaga o segredo da conexão (sem erro se já não havia).
    pub fn apagar_segredo_de_conexao(&self, nome: &str) -> Result<(), ErroDoChaveiro> {
        match self.guarda.apagar(&Self::chave_de_conexao(nome)?) {
            Ok(()) | Err(bancada_segredos::ErroDeSegredo::NaoEncontrado) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // ---------------- Credencial de estação ----------------

    /// A chave `estacao:<servidor>`, se `servidor` serve de rótulo.
    fn chave_de_estacao(servidor: &str) -> Result<String, ErroDoChaveiro> {
        if servidor.is_empty()
            || PREFIXO_DE_ESTACAO.len() + servidor.len() > TAMANHO_MAXIMO_DO_ROTULO
            || servidor.chars().any(char::is_control)
        {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        Ok(format!("{PREFIXO_DE_ESTACAO}{servidor}"))
    }

    /// Guarda (ou troca) a credencial desta estação perante `servidor` (o host do servidor).
    /// Só o pareamento chama isto.
    pub fn guardar_credencial_de_estacao(
        &self,
        servidor: &str,
        credencial: Segredo,
    ) -> Result<(), ErroDoChaveiro> {
        Ok(self
            .guarda
            .gravar(&Self::chave_de_estacao(servidor)?, credencial)?)
    }

    /// A credencial desta estação perante `servidor`, em claro. Só o portão de rede chama
    /// isto, para montar o cabeçalho de cada pedido.
    pub fn credencial_de_estacao(&self, servidor: &str) -> Result<Segredo, ErroDoChaveiro> {
        Ok(self.guarda.ler(&Self::chave_de_estacao(servidor)?)?)
    }

    /// Há credencial guardada para `servidor` (sem ler o valor).
    pub fn tem_credencial_de_estacao(&self, servidor: &str) -> bool {
        Self::chave_de_estacao(servidor).is_ok_and(|chave| self.guarda.rotulos().contains(&chave))
    }

    /// Apaga a credencial de `servidor` (estação revogada). Apagar o que não existe não é
    /// erro.
    pub fn apagar_credencial_de_estacao(&self, servidor: &str) -> Result<(), ErroDoChaveiro> {
        match self.guarda.apagar(&Self::chave_de_estacao(servidor)?) {
            Ok(()) | Err(bancada_segredos::ErroDeSegredo::NaoEncontrado) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // ---------------- O cofre.enc antigo ----------------

    /// Recusa importar se a pasta do chaveiro estiver dentro do cofre do `cofre.enc`
    /// (`<cofre>/segredos/cofre.enc`).
    fn conferir_lugar(&self, cofre_enc: &Path) -> Result<(), ErroDoChaveiro> {
        let pasta_segredos = cofre_enc.parent().unwrap_or(Path::new("."));
        garantir_fora_do_cofre(&self.pasta, pasta_segredos)?;
        if let Some(raiz) = pasta_segredos
            .parent()
            .filter(|r| !r.as_os_str().is_empty())
        {
            garantir_fora_do_cofre(&self.pasta, raiz)?;
        }
        Ok(())
    }

    /// Deixa a guarda pronta para gravar: com keyring, nada a fazer; sem keyring, cria o
    /// arquivo de fallback com a mesma senha-mestra (pedida uma vez só) ou destranca o que
    /// já existe.
    fn preparar_guarda(&self, senha_mestra: &Segredo) -> Result<(), ErroDoChaveiro> {
        match self.guarda.situacao() {
            Situacao::ChaveiroDoSistema | Situacao::ArquivoAberto => Ok(()),
            Situacao::Indisponivel => Ok(self.guarda.criar_arquivo(senha_mestra)?),
            Situacao::ArquivoTrancado => match self.guarda.destrancar(senha_mestra) {
                Ok(()) => Ok(()),
                Err(ErroDeSegredo::SenhaOuArquivoInvalido) => Err(ErroDoChaveiro::ChaveiroTrancado),
                Err(outro) => Err(outro.into()),
            },
        }
    }

    /// Importa o `cofre.enc` antigo com a senha-mestra: o CPF vai para `cpf` e cada senha
    /// para `senha:<rotulo>`. Senha errada e arquivo adulterado dão o mesmo erro. O arquivo
    /// antigo fica onde está; para apagá-lo, [`Chaveiro::conferir_importacao`].
    pub fn importar(
        &self,
        cofre_enc: &Path,
        senha_mestra: &Segredo,
    ) -> Result<ResumoDaImportacao, ErroDoChaveiro> {
        self.conferir_lugar(cofre_enc)?;
        let SegredosAntigos { cpf, senhas } = arquivo_antigo::ler(cofre_enc, senha_mestra)?;
        // Tudo validado antes da primeira gravação: nada é importado pela metade por rótulo.
        if !senhas.keys().all(|r| rotulo_de_senha_valido(r)) {
            return Err(ErroDoChaveiro::RotuloInvalido);
        }
        self.preparar_guarda(senha_mestra)?;
        let cpf_valido = cpf_valido(cpf.expor());
        if !cpf.esta_vazio() {
            self.guardar_cpf(cpf)?;
        }
        let rotulos = senhas.keys().cloned().collect();
        for (rotulo, senha) in senhas {
            if !senha.esta_vazio() {
                self.guardar_senha(&rotulo, senha)?;
            }
        }
        Ok(ResumoDaImportacao {
            cpf_valido,
            rotulos,
            onde: self.guarda.situacao(),
        })
    }

    /// Relê o `cofre.enc` e confere, valor a valor e em tempo constante, que o chaveiro
    /// guarda o mesmo. Se conferir, devolve a **oferta** de apagar o arquivo antigo; quem
    /// decide é o usuário.
    pub fn conferir_importacao(
        &self,
        cofre_enc: &Path,
        senha_mestra: &Segredo,
    ) -> Result<OfertaDeApagar, ErroDoChaveiro> {
        let SegredosAntigos { cpf, senhas } = arquivo_antigo::ler(cofre_enc, senha_mestra)?;
        let confere = |guardado: Result<Segredo, ErroDoChaveiro>, antigo: &Segredo| match guardado {
            Ok(valor) => iguais(&valor, antigo),
            Err(_) => false,
        };
        if !cpf.esta_vazio() && !confere(self.cpf(), &cpf) {
            return Err(ErroDoChaveiro::NaoConfere);
        }
        for (rotulo, senha) in &senhas {
            if !senha.esta_vazio() && !confere(self.senha(rotulo), senha) {
                return Err(ErroDoChaveiro::NaoConfere);
            }
        }
        Ok(OfertaDeApagar {
            caminho: cofre_enc.to_path_buf(),
        })
    }
}
