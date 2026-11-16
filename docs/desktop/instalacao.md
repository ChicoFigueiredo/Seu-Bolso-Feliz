# Instalar e abrir o Seu Bolso Feliz desktop

O app é um executável só, `sbf`. Funciona sem conta e sem rede. Os pacotes saem de
`scripts/desktop/empacotar-*.sh` em `dist/`.

## Instalar

**Linux (verificado)**

- `.deb`: `sudo apt install ./seu-bolso-feliz_<versao>_amd64.deb`. O apt puxa as
  dependências abaixo sozinho. Depois o atalho "Seu Bolso Feliz" aparece em Escritório.
- `.tar.gz` portátil: `tar xzf seu-bolso-feliz-<versao>-linux-x86_64.tar.gz` e rode
  `./seu-bolso-feliz-*/sbf`. As dependências abaixo ficam por sua conta.

**macOS (UNVERIFIED, escrito sem Mac)**: abra o `.dmg` ou o `.tar.gz` e arraste
"Seu Bolso Feliz.app" para Aplicativos. Sem assinatura: na primeira vez, botão direito,
Abrir, Abrir assim mesmo. Dependências: `brew install poppler qpdf rclone`.

**Windows (UNVERIFIED, escrito sem Windows)**: descompacte o `.zip` e rode `sbf.exe`.
O SmartScreen avisa porque o app não é assinado (Mais informações, Executar assim mesmo).
`pdftotext`, `qpdf` e `rclone` precisam estar no `PATH`.

## Dependências de sistema

O app chama três programas externos. Sem eles ele abre, mas perde a função indicada.

| Programa    | Pacote Debian   | Para quê                                         | No `.deb`  |
| ----------- | --------------- | ------------------------------------------------ | ---------- |
| `pdftotext` | `poppler-utils` | Texto dos PDFs, inclusive os protegidos          | Depends    |
| `qpdf`      | `qpdf`          | Decifrar PDF protegido (em memória, sem arquivo) | Depends    |
| `rclone`    | `rclone`        | Estante e cópia de segurança do cofre no Drive   | Recommends |

Para guardar segredos no chaveiro do sistema, o Linux precisa de um Secret Service
(GNOME Keyring ou KWallet). Sem ele o app usa o arquivo de fallback, cifrado por uma senha
mestra que ele pede.

## Abrir

```
sbf --demonstracao                    # dados fictícios embutidos, não toca em nada seu
sbf --inventario <pasta-do-cofre>     # o inventário de verdade, a partir do cofre
```

Na primeira abertura de um inventário vindo do Bun o app avisa da migração do banco e faz
a cópia antes. Até a primeira decisão sua, nada muda no banco.

## Onde o app guarda as coisas

A pasta de dados do app (nunca dentro do cofre):

| Sistema | Pasta                                                                |
| ------- | -------------------------------------------------------------------- |
| Linux   | `$XDG_DATA_HOME/seu-bolso-feliz` ou `~/.local/share/seu-bolso-feliz` |
| Windows | `%APPDATA%\Seu Bolso Feliz`                                          |
| macOS   | `~/Library/Application Support/Seu Bolso Feliz`                      |

- **Chaveiro**: senhas e CPF ficam no keyring do sistema (serviço "Seu Bolso Feliz"). Sem
  keyring, ficam no arquivo cifrado `segredos.cifrado` em `<pasta de dados>/chaveiro/`
  (Argon2id + XChaCha20-Poly1305). Nada de segredo vai para log, argv, banco ou arquivo
  temporário.
- **Preferências** (tema, densidade, escala): a casca as grava na mesma pasta de dados.
  UNVERIFIED: o nome exato do arquivo depende da integração da janela; confira lá.
- **Inventário e decisões**: ficam no `inventario.sqlite` do cofre, não na pasta do app.
