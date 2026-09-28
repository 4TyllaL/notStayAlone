# Segurança do !StayAlone

O !StayAlone fica o dia todo rodando no PC de quem usa. Este documento descreve o que
ele protege, de quem, e como — organizado no estilo de um *Security Target* da
**Common Criteria (ISO/IEC 15408)**: ambiente, ameaças, objetivos e requisitos
funcionais (SFRs). É uma análise inspirada no método, **não uma certificação**.

## Alvo da avaliação (TOE)

| Parte | O que faz | Toca em rede? |
|---|---|---|
| `StayAlone.exe` | mascote, lembretes, configurações, criador de mascotes | não (não tem código HTTP) |
| `stayalone-chat.exe` | plugin de conversa (API padrão OpenAI) | sim, só quando você conversa |
| Plugins de terceiros | programas que você liga na aba Plugins | o que o plugin fizer (fora do TOE) |

Dados guardados: `%APPDATA%\StayAlone\` (`config.ini`, `state.ini`, `chat.ini`,
`mascots\`) e a chave da API no Gerenciador de Credenciais do Windows.

## Hipóteses sobre o ambiente

- **A.USUARIO** — roda como usuário comum (`asInvoker`); nunca pede administrador.
- **A.SO** — Windows 10/11 atualizado; o que o próprio Windows protege (DPAPI, TLS do
  WinHTTP, isolamento entre usuários) é confiável.
- **A.CONTA** — quem já executa código com a conta do usuário está fora do escopo: esse
  atacante poderia, de qualquer forma, trocar o próprio `.exe`.

## Ameaças consideradas

| Id | Ameaça |
|---|---|
| T.MENSAGEM | Outro programa da sessão posta mensagens forjadas para as janelas do app. |
| T.MOD | Um mascote baixado da internet (`mascot.txt`/`phrases.txt`) vem malformado ou gigante. |
| T.CHAVE | A chave da API vaza (arquivo, variável de ambiente herdada, memória, rede). |
| T.REDE | Alguém na rede lê ou altera a conversa (Wi-Fi público, proxy). |
| T.SERVIDOR | Um servidor (ou IA) malicioso manda resposta gigante, aninhada ou com caracteres de controle. |
| T.DLL | Uma DLL falsa deixada na pasta do `.exe` (ex.: Downloads) é carregada no lugar da do Windows. |
| T.RECURSOS | Algo faz o app consumir CPU/memória/objetos GDI sem limite. |
| T.PRIVACIDADE | O app registra o que você digita ou envia dados sem você pedir. |
| T.PLUGIN | Um plugin é ligado sem você saber, ou o arquivo dele é trocado depois de aprovado. |

## Requisitos funcionais e como são atendidos

| SFR (família CC) | Ameaça | Como |
|---|---|---|
| **FPT_ITT / FDP_IFF** — separação de dados entre processos | T.MENSAGEM | Nenhuma mensagem do Windows carrega ponteiro. Os dados ficam no `mailbox` dentro do processo; mensagem forjada acha a caixa vazia e é ignorada (`src/mailbox.rs`). Comandos remotos (`--bolinha` etc.) só aceitam uma lista fixa. |
| **FDP_ITC.1** — importação de dados com regras | T.MOD, T.SERVIDOR | Arquivos lidos com teto de 256 KB; no máximo 64 frames e 99 mascotes; nomes/`about`/lembretes sem caracteres de controle nem de direção de texto (Unicode bidi) e com tamanho limitado; ids de pasta só `[a-z0-9-]`. |
| **FCS_STG / FDP_RIP.2** — guarda de segredos e resíduos | T.CHAVE | Chave no Gerenciador de Credenciais (DPAPI, `CRED_PERSIST_LOCAL_MACHINE`), nunca em arquivo nem variável de ambiente; campo mascarado; buffers com a chave zerados após o uso; chave validada (sem espaços/controle — impede injeção de cabeçalho HTTP). |
| **FTP_ITC.1 / FCS_TLSC** — canal confiável | T.REDE, T.CHAVE | Só HTTPS com TLS 1.2+; `http://` apenas para `localhost`/`127.0.0.1`/`[::1]`; redirecionamentos desligados (a chave não segue para outro host); certificados validados pelo WinHTTP. Conferido no app **e** no plugin. |
| **FPT_TST / FPT_FLS** — falha segura | T.SERVIDOR | Resposta HTTP até 64 KB (uma conversa tem 1–3 KB); pedido até 64 KB; JSON com até 64 níveis (não estoura a pilha); `max_tokens` com teto de 4096; saída de qualquer plugin até 16 KB e processo encerrado após 120 s. |
| **FPT (proteção do TSF)** | T.DLL | `winhttp.dll` carregada só de System32 (`LOAD_LIBRARY_SEARCH_SYSTEM32`); as demais DLLs importadas são "KnownDLLs". Binários com ASLR (alta entropia) e DEP; manifesto `asInvoker`. Plugin personalizado só `.exe` com caminho absoluto (nada de `.bat`/PATH). |
| **FDP_ACF.1 / FPT_TST.1** — controle de acesso e integridade dos plugins | T.PLUGIN | Plugin novo chega desligado; ligar exige sua confirmação (padrão "Não") e grava o SHA-256 do programa. Antes de **cada** execução o hash é conferido; se mudou, não roda e o mascote avisa. No `config.ini`, plugin de terceiros sem hash válido é ignorado. O programa precisa ser `.exe`/`.ps1` **dentro** da pasta do plugin (sem caminhos nem `..`); o PowerShell usado é o de System32. |
| **FRU_RSA.1** — cotas de recursos | T.RECURSOS | Timers ligados só quando necessários; janelas, fontes e bitmaps criados sob demanda e liberados (sem vazamento GDI/USER medido); até 50 lembretes seus; histórico da conversa com 12 mensagens. |
| **FPR_UNO / FDP_IFC.1** — privacidade e fluxo | T.PRIVACIDADE | Sem hooks de teclado/mouse; só `GetLastInputInfo` (quando, nunca o quê). Nada sai do PC sem você configurar a conversa; o app não tem telemetria nem logs. |

## Riscos residuais (o que ainda não está coberto)

- **Sem assinatura digital (Authenticode).** Para distribuir, assine os dois `.exe`: o
  Windows SmartScreen confia mais e dá para detectar adulteração.
- **Sem Control Flow Guard.** A toolchain GNU não gera CFG; compilar com MSVC
  (`-C control-flow-guard`) adiciona essa proteção.
- **Plugins sem sandbox.** Um plugin ligado roda com as permissões do usuário: a
  aprovação garante que é *o arquivo que você aprovou*, não que ele seja bom. Ligue só
  plugins de quem você confia. Scripts `.ps1` rodam com `-ExecutionPolicy Bypass`
  (a aprovação por hash faz o papel da política de execução).
- **Conteúdo da IA.** A resposta é texto limpo e cortado, mas não passa por filtro de
  conteúdo; o provedor escolhido é quem modera.
- **Atacante com a conta do usuário** (A.CONTA) pode ler a credencial, como qualquer
  programa do próprio usuário — é o limite do modelo de segurança do Windows.

## Como verificar

```bash
cargo test --workspace
```

Os testes cobrem, entre outros: mensagens forjadas (`mailbox`), endereços HTTP proibidos,
nomes/chaves inválidos, arquivos grandes demais, JSON profundo, limite de tokens e
limpeza de textos. Também: impressão digital de plugins, programas fora da pasta do plugin e SHA-256 (vetores oficiais). `cargo test -- --ignored` testa o Gerenciador de Credenciais de verdade.

## Reportar um problema

Encontrou uma falha? Abra uma *issue* sem detalhes exploráveis e peça um contato privado,
ou fale diretamente com o mantenedor.
