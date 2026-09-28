//! Idioma da interface: português (o texto no código) ou inglês (a tabela `EN`).
//!
//! Os textos ficam escritos em português no próprio código, dentro de `tr("...")`;
//! em inglês, `tr` troca pela tradução da tabela. Um teste confere que todo
//! `tr("...")` do código tem tradução — texto novo sem tradução quebra o build.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::Language;

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Escolhe o idioma ("Igual ao Windows" segue o idioma da interface do Windows).
pub fn set(language: Language) {
    let english = match language {
        Language::Portuguese => false,
        Language::English => true,
        Language::Auto => !windows_is_portuguese(),
    };
    ENGLISH.store(english, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

fn windows_is_portuguese() -> bool {
    const LANG_PORTUGUESE: u16 = 0x16;
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3ff == LANG_PORTUGUESE
}

/// O texto no idioma atual.
pub fn tr(pt: &'static str) -> &'static str {
    if !is_english() {
        return pt;
    }
    EN.iter().find(|(p, _)| *p == pt).map_or(pt, |(_, en)| en)
}

/// Troca cada `{}` de `template` pelo próximo de `args`.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out += &rest[..i];
        if let Some(arg) = args.next() {
            out += &arg.to_string();
        }
        rest = &rest[i + 2..];
    }
    out + rest
}

/// (português, inglês).
const EN: &[(&str, &str)] = &[
    ("Geral", "General"),
    ("Lembretes", "Reminders"),
    ("Conversa", "Chat"),
    ("Criar mascote", "Make a mascot"),
    ("Galeria", "Gallery"),
    ("Devagar", "Slow"),
    ("Normal", "Normal"),
    ("Rápido", "Fast"),
    ("Muito rápido", "Very fast"),
    ("Gemini (Google) — recomendado", "Gemini (Google) — recommended"),
    ("Ollama (roda no seu PC, sem chave)", "Ollama (runs on your PC, no key)"),
    ("Configurações — !StayAlone", "Settings — !StayAlone"),
    ("Seu mascote", "Your mascot"),
    ("Mascote", "Mascot"),
    ("Abrir pasta", "Open folder"),
    ("Amigo na tela", "Buddy on screen"),
    ("Tamanho", "Size"),
    ("Velocidade ao andar", "Walking speed"),
    ("Comportamento", "Behavior"),
    ("Considerar ausente após", "Consider me away after"),
    ("min sem usar o PC", "min without using the PC"),
    ("Seu aniversário", "Your birthday"),
    ("o mascote comemora com você", "your mascot celebrates with you"),
    ("Iniciar junto com o Windows", "Start with Windows"),
    ("Procurar versões novas (uma vez por dia, no GitHub)", "Check for new versions (once a day, on GitHub)"),
    ("Ficar quieto em reuniões (Teams, Zoom, Webex...)", "Stay quiet in meetings (Teams, Zoom, Webex...)"),
    ("Ele olha só o nome do programa aberto, nunca o que está na tela.", "It only looks at the name of the open program, never at what's on screen."),
    ("Aparência", "Appearance"),
    ("Tema", "Theme"),
    ("Idioma", "Language"),
    ("Seus lembretes", "Your reminders"),
    ("Marque para ativar. Eles contam só o tempo em que você está usando o PC.", "Check to turn on. They only count the time you're using the PC."),
    ("Lembrete", "Reminder"),
    ("A cada", "Every"),
    ("Texto", "Text"),
    ("Ex.: Conferir o e-mail", "E.g.: Check email"),
    ("a cada", "every"),
    ("Adicionar", "Add"),
    ("Salvar alteração", "Save change"),
    ("Remover", "Remove"),
    ("Conversar com o mascote", "Chat with your mascot"),
    ("Serviços no padrão da API da OpenAI. Sem chave, nada sai do seu PC.", "Services using the OpenAI API format. Without a key, nothing leaves your PC."),
    ("Serviço", "Service"),
    ("Endereço da API", "API address"),
    ("Modelo", "Model"),
    ("Nome da chave", "Key name"),
    ("Chave da API", "API key"),
    ("Colar a chave", "Paste the key"),
    ("cole aqui para salvar", "paste here to save"),
    ("Remover chave", "Remove key"),
    ("Criar chave", "Get a key"),
    ("Fica no Gerenciador de Credenciais do Windows, protegida pela sua conta, e só vai por HTTPS ao serviço escolhido.", "It stays in Windows Credential Manager, protected by your account, and only goes over HTTPS to the chosen service."),
    ("Testar conversa", "Test chat"),
    ("Memória", "Memory"),
    ("Lembrar do que eu contar na conversa (fica só neste PC)", "Remember what I tell it in chat (stays on this PC)"),
    ("Ver e editar", "View and edit"),
    ("Esquecer tudo", "Forget everything"),
    ("Clique pinta • botão direito apaga • duplo clique numa cor troca a cor. A cor com ponto branco são os olhos.", "Click paints • right-click erases • double-click a color to change it. The color with a white dot is the eyes."),
    ("Nome", "Name"),
    ("Quem é (personalidade)", "Who it is (personality)"),
    ("Ex.: um polvo roxo curioso", "E.g.: a curious purple octopus"),
    ("Começar a partir de", "Start from"),
    ("Limpar", "Clear"),
    ("Espelhar", "Mirror"),
    ("Salvar e usar", "Save and use"),
    ("Pose que você está desenhando", "Pose you're drawing"),
    ("Poses em branco são criadas sozinhas a partir da pose parada (piscar, dormir, andar, pular).", "Blank poses are made automatically from the standing pose (blink, sleep, walk, jump)."),
    ("Ou peça para a IA desenhar", "Or ask the AI to draw it"),
    ("Ex.: um polvo roxo de chapéu de marinheiro", "E.g.: a purple octopus in a sailor hat"),
    ("Criar com IA", "Create with AI"),
    ("Plugins instalados", "Installed plugins"),
    ("Programas que dão novos poderes ao mascote. Plugins novos chegam desligados: ligue só os de quem você confia.", "Programs that give your mascot new powers. New plugins arrive turned off: only turn on the ones you trust."),
    ("Tipo", "Type"),
    ("Situação", "Status"),
    ("Testar", "Test"),
    ("Abrir pasta de plugins", "Open plugins folder"),
    ("Atualizar lista", "Refresh list"),
    ("Crie o seu com um .exe ou um script PowerShell: veja o LEIA-ME.txt e o exemplo \"Curiosidades\" na pasta de plugins.", "Make your own with an .exe or a PowerShell script: see LEIA-ME.txt and the \"Curiosidades\" example in the plugins folder."),
    ("Mascotes da comunidade", "Community mascots"),
    ("Feitos pela comunidade: só desenho e falas, nada que rode no seu PC. Conferidos (SHA-256) antes de instalar.", "Made by the community: just drawings and lines, nothing that runs on your PC. Checked (SHA-256) before installing."),
    ("Autor", "Author"),
    ("Instalar", "Install"),
    ("Salvar", "Save"),
    ("Cancelar", "Cancel"),
    ("Buscando a galeria...", "Loading the gallery..."),
    ("Selecione um item para ver o que ele faz.", "Select an item to see what it does."),
    ("Nenhum", "None"),
    ("Igual ao Windows", "Same as Windows"),
    ("Claro", "Light"),
    ("Escuro", "Dark"),
    ("Personalizado", "Custom"),
    ("Ele (o mascote)", "He"),
    ("Ela (a mascote)", "She"),
    ("Desenho em branco", "Blank drawing"),
    ("Intervalo", "Interval"),
    ("Escreva o texto do lembrete.", "Write the reminder text."),
    ("Este serviço não usa chave.", "This service doesn't use a key."),
    ("Use só letras, números e _ no nome.", "Use only letters, numbers and _ in the name."),
    ("✓ Chave salva.", "✓ Key saved."),
    ("Nenhuma chave salva ainda.", "No key saved yet."),
    ("O endereço da API precisa começar com https://\n(http:// só para serviços no seu próprio PC, como o Ollama).", "The API address must start with https://\n(http:// only for services on your own PC, like Ollama)."),
    ("O nome da chave só pode ter letras, números e _ (ex.: GEMINI_API_KEY).", "The key name can only have letters, numbers and _ (e.g. GEMINI_API_KEY)."),
    ("Testando...", "Testing..."),
    ("ligado", "on"),
    ("desligado", "off"),
    ("arquivo mudou", "file changed"),
    ("falta o arquivo", "file missing"),
    ("Selecione um plugin para ver os detalhes.", "Select a plugin to see the details."),
    ("\nO arquivo mudou depois que você ligou: marque de novo só se confiar na nova versão.", "\nThe file changed after you turned it on: check it again only if you trust the new version."),
    ("\nO arquivo do plugin não está mais na pasta.", "\nThe plugin file is no longer in the folder."),
    ("Não consegui ler o arquivo do plugin.", "I couldn't read the plugin file."),
    ("Ligue o plugin para testar.", "Turn the plugin on to test it."),
    ("Descreva o mascote que você quer (ex.: um polvo roxo de chapéu).", "Describe the mascot you want (e.g. a purple octopus in a hat)."),
    ("Desenhando... isso pode levar alguns segundos.", "Drawing... this may take a few seconds."),
    ("Dê um nome para o seu mascote.", "Give your mascot a name."),
    ("Desenhe alguma coisa primeiro (ou comece a partir de um mascote).", "Draw something first (or start from a mascot)."),
    ("Pronto! Ajuste o desenho se quiser e clique em Salvar e usar.", "Done! Tweak the drawing if you like and click Save and use."),
    ("✓ Rodou, mas não tinha nada para falar desta vez.", "✓ It ran, but had nothing to say this time."),
    ("Instalado! Escolha o mascote na página Geral (ou no painel).", "Installed! Pick the mascot in the General page (or in the panel)."),
    ("Ainda não lembra de nada.", "Doesn't remember anything yet."),
    ("Lembra de 1 coisa.", "Remembers 1 thing."),
    ("Esquecer tudo o que o mascote lembra de você?", "Forget everything your mascot remembers about you?"),
    ("Tempo para considerar ausente", "Time to consider you away"),
    ("Aniversário: use dia/mês, por exemplo 25/12.", "Birthday: use day/month, for example 25/12."),
    ("Configurações", "Settings"),
    ("Sobre", "About"),
    ("Sobre · v{}", "About · v{}"),
    ("Um mascote em pixel art que faz companhia na área de trabalho: lembra de beber água e fazer pausas, conversa com você e fica levinho, feito direto na API do Windows.", "A pixel-art pet that keeps you company on your desktop: it reminds you to drink water and take breaks, chats with you and stays light, built straight on the Windows API."),
    ("Versão", "Version"),
    ("Lançada em", "Released on"),
    ("Licença", "License"),
    ("Meus projetos", "My projects"),
    ("Página no GitHub", "GitHub page"),
    ("Procurar atualização", "Check for updates"),
    ("Procurando... o mascote avisa o que encontrar.", "Checking... your mascot will tell you what it finds."),
    ("Você já está na versão mais nova (v{})!", "You're already on the newest version (v{})!"),
    ("Não consegui procurar agora: {}", "I couldn't check right now: {}"),
    ("Instalando \"{}\"...", "Installing \"{}\"..."),
    ("{}: use um número entre {} e {}.", "{}: use a number between {} and {}."),
    ("Dá para ter até {} lembretes seus.", "You can have up to {} reminders of your own."),
    ("✓ Usando a variável de ambiente {}.", "✓ Using the environment variable {}."),
    ("Não salvei a chave: {}", "I didn't save the key: {}"),
    ("  •  fala a cada {} min", "  •  speaks every {} min"),
    ("Arquivo:", "File:"),
    ("O arquivo do plugin \"{}\" não está mais na pasta.", "The file of plugin \"{}\" is no longer in the folder."),
    ("Ligar o plugin \"{}\"?\n\n{}\n\nEle é um programa ({}) que vai rodar no seu PC com as suas permissões. Ligue só plugins de quem você confia.", "Turn on plugin \"{}\"?\n\n{}\n\nIt is a program ({}) that will run on your PC with your permissions. Only turn on plugins from people you trust."),
    ("Testando \"{}\"...", "Testing \"{}\"..."),
    ("Não consegui abrir esse mascote.\n\n{}", "I couldn't open this mascot.\n\n{}"),
    ("Já existe um mascote chamado \"{}\". Substituir?", "There's already a mascot called \"{}\". Replace it?"),
    ("Não consegui salvar o mascote.\n\n{}", "I couldn't save the mascot.\n\n{}"),
    ("Salvo! A {} já está na sua área de trabalho.", "Saved! {} is on your desktop now."),
    ("Salvo! O {} já está na sua área de trabalho.", "Saved! {} is on your desktop now."),
    ("✓ Funcionou! \"{}\"", "✓ It works! \"{}\""),
    ("Não deu: {}", "That didn't work: {}"),
    ("✓ Respondeu: \"{}\"", "✓ It answered: \"{}\""),
    ("{} mascotes na galeria.", "{} mascots in the gallery."),
    ("Não consegui abrir a galeria: {}", "I couldn't open the gallery: {}"),
    ("Não instalei: {}", "Not installed: {}"),
    ("Lembra de {} coisas.", "Remembers {} things."),
    ("Responda em uma frase curta, em português.", "Answer in one short sentence, in English."),
    ("Diga oi!", "Say hi!"),
    ("1 ligado", "1 on"),
    ("1 pausa", "1 break"),
    ("1 vez", "1 glass of water"),
    ("Abrir junto com o Windows", "Open with Windows"),
    ("Alongar", "Stretch"),
    ("Andando", "Walking"),
    ("Anotado!", "Noted!"),
    ("Atualizei! Agora estou na versão {}.", "Updated! I'm now on version {}."),
    ("Avisos", "Notices"),
    ("Baixando a versão {}...", "Downloading version {}..."),
    ("Beber água", "Drink water"),
    ("Bebi água", "Drank water"),
    ("Bem-vindo — !StayAlone", "Welcome — !StayAlone"),
    ("Bolinha", "Ball"),
    ("Cole a chave aqui", "Paste the key here"),
    ("Comendo", "Eating"),
    ("Começar!", "Let's go!"),
    ("Conversa com IA (nativo)", "AI chat (built-in)"),
    ("Conversar", "Chat"),
    ("Criar uma chave grátis", "Get a free key"),
    ("Descansar os olhos", "Rest your eyes"),
    ("Dica: clique no mascote para fazer carinho, arraste para carregar e use o botão direito para abrir o painel.", "Tip: click the mascot to pet it, drag to carry it and right-click to open the panel."),
    ("Diga algo para a {}...", "Say something to {}..."),
    ("Diga algo para o {}...", "Say something to {}..."),
    ("Dormindo", "Sleeping"),
    ("Dá para trocar depois pelo painel (botão direito no mascote).", "You can switch later in the panel (right-click the mascot)."),
    ("Ela fica no Gerenciador de Credenciais do Windows, nunca em arquivo. Pode pular: dá para fazer isso depois em Configurações → Conversa.", "It stays in Windows Credential Manager, never in a file. You can skip this and do it later in Settings → Chat."),
    ("Esconder o mascote", "Hide the mascot"),
    ("Extra pequeno", "Extra small"),
    ("Feliz", "Happy"),
    ("Foco", "Focus"),
    ("G", "L"),
    ("GitHub respondeu HTTP {}", "GitHub answered HTTP {}"),
    ("Grande", "Large"),
    ("Guardar", "Put away"),
    ("Hmm, não consegui responder: {}", "Hmm, I couldn't answer: {}"),
    ("Hoje: {juntos} juntos · {pausas}", "Today: {juntos} together · {pausas}"),
    ("Lembrar de alongar (a cada 60 min)", "Remind me to stretch (every 60 min)"),
    ("Lembrar de beber água (a cada 45 min)", "Remind me to drink water (every 45 min)"),
    ("Lembrar de descansar os olhos (a cada 20 min)", "Remind me to rest my eyes (every 20 min)"),
    ("Mascote da IA", "AI mascot"),
    ("Médio", "Medium"),
    ("Não consegui atualizar: {}", "I couldn't update: {}"),
    ("Não consegui carregar '{}'.\n\n{}", "I couldn't load '{}'.\n\n{}"),
    ("Não consegui carregar o mascote '{}', usando o padrão.\n\n{}", "I couldn't load the mascot '{}', using the default one.\n\n{}"),
    ("P", "S"),
    ("Para conversar comigo, ligue um plugin de conversa em Configurações → Plugins.", "To chat with me, turn on a chat plugin in Settings → Plugins."),
    ("Parado (a base)", "Standing (the base)"),
    ("Parar", "Stop"),
    ("Pequeno", "Small"),
    ("Petisco", "Snack"),
    ("Próximo", "Next"),
    ("Responde com a IA escolhida na aba Conversa (Gemini, OpenAI, Ollama...).", "Answers with the AI chosen in the Chat tab (Gemini, OpenAI, Ollama...)."),
    ("Resumo do dia", "Today's summary"),
    ("Sair", "Quit"),
    ("Silenciar por 1 hora", "Mute for 1 hour"),
    ("Tem versão nova de mim (v{})! Abra o painel para atualizar.", "There's a new version of me (v{})! Open the panel to update."),
    ("Versão {} disponível — atualizar", "Version {} available — update"),
    ("Voltar", "Back"),
    ("a IA não mandou um desenho válido. Tente de novo ou descreva de outro jeito.", "the AI didn't send a valid drawing. Try again or describe it another way."),
    ("a chave da API foi recusada — confira em Configurações → Conversa.", "the API key was refused — check it in Settings → Chat."),
    ("a chave da API tem caracteres inválidos — cole de novo em Configurações → Conversa.", "the API key has invalid characters — paste it again in Settings → Chat."),
    ("a conversa não veio em UTF-8.", "the chat didn't come in UTF-8."),
    ("a conversa veio grande demais.", "the chat was too big."),
    ("a galeria respondeu HTTP {}", "the gallery answered HTTP {}"),
    ("a resposta do servidor veio grande demais.", "the server's answer was too big."),
    ("api_key_env inválido no chat.ini (use letras, números e _).", "invalid api_key_env in chat.ini (use letters, numbers and _)."),
    ("arquivo de atualização fora do lugar esperado", "update file is not where it should be"),
    ("chat.ini sem api_base ou model.", "chat.ini has no api_base or model."),
    ("cole a chave aqui (opcional)", "paste the key here (optional)"),
    ("demorou demais e foi encerrado.", "took too long and was stopped."),
    ("downloads só por https://", "downloads only over https://"),
    ("endereço de atualização recusado", "update address refused"),
    ("endereço inválido", "invalid address"),
    ("erro de rede {}.", "network error {}."),
    ("erro no servidor", "server error"),
    ("esse item não está mais na galeria", "this item is no longer in the gallery"),
    ("falha na conexão segura (HTTPS): certificado ou protocolo recusado.", "secure connection (HTTPS) failed: certificate or protocol refused."),
    ("isso não parece uma chave de API (sem espaços, de 8 a 512 caracteres).", "this doesn't look like an API key (no spaces, 8 to 512 characters)."),
    ("muitas mensagens seguidas (limite da API). Tenta daqui a pouco!", "too many messages in a row (API limit). Try again in a bit!"),
    ("nenhum hoje", "none today"),
    ("nenhum ligado", "none on"),
    ("nenhuma pausa", "no breaks"),
    ("nenhuma vez", "no water"),
    ("não consegui abrir o programa ({})", "I couldn't open the program ({})"),
    ("não consegui baixar {} (HTTP {})", "I couldn't download {} (HTTP {})"),
    ("não consegui instalar a versão nova ({})", "I couldn't install the new version ({})"),
    ("não consegui trocar o programa nesta pasta ({})", "I couldn't replace the program in this folder ({})"),
    ("não encontrei o WinHTTP do Windows.", "I couldn't find Windows' WinHTTP."),
    ("não li a conversa: {}", "I couldn't read the chat: {}"),
    ("o Windows não deixou guardar a chave.", "Windows didn't let me save the key."),
    ("o arquivo baixado não confere com o da release (SHA-256)", "the downloaded file doesn't match the release (SHA-256)"),
    ("o arquivo baixado não é um programa do Windows", "the downloaded file isn't a Windows program"),
    ("a versão nova não veio com a assinatura do !StayAlone", "the new version doesn't carry the !StayAlone signature"),
    ("a assinatura da versão nova não confere; a atualização foi recusada", "the new version's signature doesn't match; the update was refused"),
    ("o download falhou (HTTP {})", "the download failed (HTTP {})"),
    ("o endereço precisa começar com https://", "the address must start with https://"),
    ("o modelo não respondeu nada (talvez max_tokens baixo demais).", "the model didn't answer anything (maybe max_tokens is too low)."),
    ("o nome da chave só pode ter letras, números e _.", "the key name can only have letters, numbers and _."),
    ("o programa falhou", "the program failed"),
    ("o servidor demorou demais para responder.", "the server took too long to answer."),
    ("por segurança, http:// só é aceito para serviços no seu PC (use https://).", "for safety, http:// is only accepted for services on your PC (use https://)."),
    ("porta inválida no endereço", "invalid port in the address"),
    ("resposta inesperada do servidor (HTTP {})", "unexpected answer from the server (HTTP {})"),
    ("resposta inválida", "invalid answer"),
    ("sem conexão com a internet (ou com o servidor).", "no connection to the internet (or to the server)."),
    ("sem pasta de dados", "no data folder"),
    ("um mascote fofinho", "a cute little mascot"),
    ("{} hoje", "{} today"),
    ("{} inválido, usando o padrão.\n\n{}", "{} is invalid, using the default.\n\n{}"),
    ("{} ligados", "{} on"),
    ("{} não confere com o índice (SHA-256)", "{} doesn't match the index (SHA-256)"),
    ("{} pausas", "{} breaks"),
    ("{} vezes", "{} glasses of water"),
    ("índice inválido", "invalid index"),
    ("o arquivo do plugin mudou e ele foi pausado. Ligue de novo em Configurações → Plugins se confiar na nova versão.", "the plugin file changed and it was paused. Turn it on again in Settings → Plugins if you trust the new version."),
    ("Oi! Eu sou o !StayAlone", "Hi! I'm !StayAlone"),
    ("Um mascote que te faz companhia enquanto você usa o PC. Escolha quem vai morar na sua área de trabalho:", "A mascot that keeps you company while you use your PC. Choose who's going to live on your desktop:"),
    ("Como eu posso ajudar", "How I can help"),
    ("Lembretes gentis, contados só enquanto você usa o PC. Dá para mudar tudo depois nas Configurações.", "Gentle reminders, counted only while you use the PC. You can change everything later in Settings."),
    ("Quer conversar comigo?", "Want to chat with me?"),
    ("Opcional: com uma chave gratuita do Google Gemini você conversa com o mascote e ele até desenha mascotes novos.", "Optional: with a free Google Gemini key you can chat with your mascot, and it can even draw new mascots."),
    ("falta a chave da API: cole em Configurações → Conversa.", "the API key is missing: paste it in Settings → Chat."),
    ("Meu mascote", "My mascot"),
    ("um mascote fofinho feito à mão", "a cute handmade mascot"),
    ("mascote não encontrado", "mascot not found"),
    ("mascot.txt não abriu (ou passa de 256 KB)", "mascot.txt didn't open (or is over 256 KB)"),
    ("não encontrado", "not found"),
    ("É", "Pronoun"),
    ("(Clique e eu te guio!)", "(Click me and I'll guide you!)"),
    ("Acessórios de época (Natal, Halloween, aniversário...)", "Seasonal accessories (Christmas, Halloween, birthday...)"),
    ("Ctrl+Alt+M abre a conversa de qualquer lugar", "Ctrl+Alt+M opens the chat from anywhere"),
    ("Durante o foco, os lembretes esperam a pausa", "During focus, reminders wait for the break"),
    ("Foco (pomodoro)", "Focus (pomodoro)"),
    ("Meta de água", "Water goal"),
    ("Pausa do foco", "Focus break"),
    ("copos por dia (0 = sem meta)", "glasses a day (0 = no goal)"),
    ("min, pausa de", "min, break of"),
    ("Água e foco", "Water and focus"),
    ("Olhe para algo bem longe, pela janela se der... {}", "Look at something far away, out the window if you can... {}"),
    ("Agora pisque devagar, várias vezes... {}", "Now blink slowly, a few times... {}"),
    ("Gire os ombros para trás, devagar... {}", "Roll your shoulders back, slowly... {}"),
    ("Incline a cabeça para a direita... {}", "Tilt your head to the right... {}"),
    ("Agora para a esquerda... {}", "Now to the left... {}"),
    ("Estique os braços para cima, bem alto! {}", "Stretch your arms up, nice and high! {}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Literais dentro de `tr("...")` num trecho de código (sem as partes de teste).
    fn literals(src: &str) -> Vec<String> {
        let code = src.split("#[cfg(test)]").next().unwrap_or(src);
        let mut out = Vec::new();
        let mut rest = code;
        while let Some(i) = rest.find("tr(\"") {
            if rest[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                rest = &rest[i + 4..];
                continue;
            }
            let bytes = &rest.as_bytes()[i + 4..];
            let (mut text, mut j) = (Vec::new(), 0);
            while bytes[j] != b'"' {
                if bytes[j] == b'\\' {
                    j += 1;
                    match bytes[j] {
                        b'n' => text.push(b'\n'),
                        // "\" no fim da linha: continua na próxima, sem os espaços do começo.
                        b'\r' | b'\n' => {
                            while bytes[j + 1].is_ascii_whitespace() {
                                j += 1;
                            }
                        }
                        other => text.push(other),
                    }
                } else {
                    text.push(bytes[j]);
                }
                j += 1;
            }
            out.push(String::from_utf8(text).unwrap());
            rest = &rest[i + 4 + j..];
        }
        out
    }

    fn sources(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("lang.rs") {
                out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
            }
        }
    }

    #[test]
    fn every_text_has_an_english_translation() {
        let mut files = Vec::new();
        sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut missing = Vec::new();
        let mut used = std::collections::HashSet::new();
        for (file, src) in &files {
            for text in literals(src) {
                if !EN.iter().any(|(pt, _)| *pt == text) {
                    missing.push(format!("{file}: {text:?}"));
                }
                used.insert(text);
            }
        }
        assert!(missing.is_empty(), "sem tradução:\n{}", missing.join("\n"));
        // Listas constantes (traduzidas onde são usadas) contam como uso também.
        let quoted = |pt: &str| format!("\"{}\"", pt.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"));
        let in_code = |pt: &str| files.iter().any(|(_, src)| src.contains(&quoted(pt)));
        let unused: Vec<_> = EN.iter().filter(|(pt, _)| !used.contains(*pt) && !in_code(pt)).map(|(pt, _)| pt).collect();
        assert!(unused.is_empty(), "traduções sem uso: {unused:?}");
    }

    #[test]
    fn translations_keep_the_placeholders() {
        for (pt, en) in EN {
            assert_eq!(pt.matches("{}").count(), en.matches("{}").count(), "{pt}");
            let mut seen = 0;
            for (other, _) in EN {
                seen += (other == pt) as usize;
            }
            assert_eq!(seen, 1, "repetido: {pt}");
        }
    }

    #[test]
    fn fill_replaces_in_order() {
        assert_eq!(fill("{} de {}", &[&1, &"dois"]), "1 de dois");
        assert_eq!(fill("sem nada", &[&1]), "sem nada");
    }
}
