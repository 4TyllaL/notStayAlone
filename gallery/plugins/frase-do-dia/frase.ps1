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

if ($pedido.hora -ge 22 -or $pedido.hora -lt 6) {
    "Já está tarde... que tal guardar a próxima ideia para amanhã?"
} else {
    Get-Random -InputObject $frases
}
