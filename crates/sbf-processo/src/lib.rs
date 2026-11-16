//! Como o Seu Bolso Feliz chama um programa externo (ADR-012: uma implementação por regra).
//! Antes ela estava copiada, e já divergindo, no `sbf-pdf` (`pdftotext`, `qpdf`), no
//! `sbf-estante` (`rclone`) e no `sbf-disco` (`df`, `diskutil`, `fdesetup`, `powershell.exe`).
//! A regra vem dos achados A6, A7 e A10 da revisão de segurança da fatia 1:
//!
//! - **Caminho absoluto, resolvido uma vez**, nas pastas do sistema ([`pastas_do_sistema`]),
//!   nunca pelo `PATH` do usuário: um executável com o mesmo nome plantado em `~/.bun/bin`,
//!   num shim ou em `/usr/local/sbin` receberia a senha candidata, o PDF decifrado ou a senha
//!   ofuscada da cópia ([`localizar`]).
//! - **Nada em pasta gravável por outros**: o arquivo e a pasta dele (depois de seguir links)
//!   não podem ter escrita para "outros", nem para o grupo se o grupo não for o do root nem o
//!   grupo próprio do dono ([`confiavel`]). Vale também para o caminho configurado à mão.
//! - **Ambiente mínimo**: o filho não herda tokens nem chaves de API do shell; recebe só um
//!   `PATH` do sistema, o idioma e, no Windows, as variáveis sem as quais um processo não sobe
//!   ([`VARIAVEIS_BASICAS`]). Quem precisa de mais (o rclone, que acha a configuração pelo
//!   `HOME`) diz quais nomes herdar ([`Chamada::herdando`]).
//! - **Prazo e limite de saída**: todo filho tem prazo (ou declara que não tem, como as
//!   transferências do rclone) e é morto ao passar; a saída guardada tem teto e o resto é
//!   drenado, para o filho não travar com o pipe cheio ([`Chamada::rodar`]).
//! - **O que é sensível vai pela entrada padrão ou pelo ambiente do filho**, nunca pelo argv
//!   (que aparece no `ps`), e em buffers zerados ao descartar.
//!
//! [`abrir_no_sistema`] abre um arquivo (ou a pasta dele) com o programa padrão do sistema
//! (`xdg-open`, `open`, `explorer`) sob a mesma regra.

mod chamada;
mod programa;
mod sistema;

pub use chamada::{
    Chamada, ERRO_MAXIMO_PADRAO, FalhaDeProcesso, Lido, PRAZO_PADRAO, SAIDA_MAXIMA_PADRAO, Saida,
    comando, escrever_e_fechar, esperar, ler_com_limite, ler_o_fim,
};
pub use programa::{
    PASTAS_DO_SISTEMA, VARIAVEIS_BASICAS, ambiente_minimo, confiavel, localizar, localizar_ou_nome,
    pastas_do_sistema,
};
pub use sistema::abrir_no_sistema;
