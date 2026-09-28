# Frase do dia: o mascote diz uma frase de ânimo de tempos em tempos.
[Console]::InputEncoding  = [Text.Encoding]::UTF8
[Console]::OutputEncoding = [Text.Encoding]::UTF8

$pedido = [Console]::In.ReadToEnd() | ConvertFrom-Json

$frases = @(
    "Um passo de cada vez já é andar pra frente.",
    "Você não precisa dar conta de tudo hoje.",
    "Feito é melhor que perfeito.",
    "Respira fundo: você já passou por dias mais difíceis.",
    "Cada pausa também é progresso.",
    "Pequenas vitórias contam. Comemora essa!",
    "Seja gentil com você mesmo hoje.",
    "O que você está fazendo agora importa."
)

$phrases = @(
    "One step at a time is still moving forward.",
    "You don't have to get everything done today.",
    "Done is better than perfect.",
    "Take a deep breath: you've been through harder days.",
    "Every break is progress too.",
    "Small wins count. Celebrate this one!",
    "Be kind to yourself today.",
    "What you're doing right now matters."
)

# "idioma" é "pt" ou "en" (o idioma escolhido no app).
$english = $pedido.idioma -eq "en"
if ($pedido.hora -ge 22 -or $pedido.hora -lt 6) {
    if ($english) { "It's getting late... how about saving the next idea for tomorrow?" }
    else { "Já está tarde... que tal guardar a próxima ideia para amanhã?" }
} elseif ($english) {
    Get-Random -InputObject $phrases
} else {
    Get-Random -InputObject $frases
}
