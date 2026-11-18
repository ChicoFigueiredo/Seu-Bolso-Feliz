//! O OCR de imagens do Seu Bolso Feliz (prints de app, fotos de comprovante).
//!
//! - [`Ocr`]: o `tesseract` pela regra única do `sbf-processo` (ADR-012): caminho absoluto
//!   nas pastas do sistema, ambiente mínimo (o filho não herda nenhum token), prazo de 30 s,
//!   saída guardada até 200 KB. **A imagem vai pela entrada padrão**, nunca pelo argv, e o
//!   tesseract não fala com a rede. Não há cópia da imagem em disco.
//! - Antes do tesseract a imagem é preparada ([`preparar`]): cinza, **invertida se o fundo é
//!   escuro** (print de app em tela escura) e ampliada 2x se for pequena. A segmentação é
//!   `--psm 6` (um bloco uniforme de texto): numa medida com um print escuro e pequeno, o
//!   `psm 4` e o `psm 6` empataram, e o `6` mantém a ordem das linhas de um cartão de app.
//! - [`ocr_do_achado`]: o que o reconhecimento de um arquivo e o worker fazem com uma imagem
//!   nova, gravando em `texto` com o método [`METODO_OCR`] e reaproveitando o que já existe.
//!
//! Sem `tesseract` no sistema o OCR é "indisponível": não é erro e nada é gravado, para
//! tentar de novo quando ele for instalado.

mod achado;
mod leitura;

pub use achado::{DesfechoDoOcr, ocr_do_achado};
pub use leitura::{
    EXTENSOES_DE_IMAGEM, FalhaDoOcr, IDIOMA_PADRAO, LIMITE_DA_IMAGEM, LIMITE_DA_SAIDA, Ocr,
    PRAZO_DO_OCR, e_imagem, preparar,
};
pub use sbf_inventario::texto::{METODO_OCR_SEM_TEXTO, METODO_OCR_TESSERACT as METODO_OCR};
