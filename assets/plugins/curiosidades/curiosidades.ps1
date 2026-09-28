# Plugin de exemplo do !StayAlone: de vez em quando o mascote conta uma curiosidade.
# O app manda um JSON no stdin; o que este script escrever, o mascote fala.
[Console]::InputEncoding  = [Text.Encoding]::UTF8
[Console]::OutputEncoding = [Text.Encoding]::UTF8

$pedido = [Console]::In.ReadToEnd() | ConvertFrom-Json

$curiosidades = @(
    "o polvo tem três corações e sangue azul.",
    "as lontras-marinhas dormem de mãos dadas para não se separarem.",
    "o mel quase não estraga: já acharam mel comestível em tumbas egípcias.",
    "os flamingos ficam cor-de-rosa por causa do que comem.",
    "uma nuvem comum pode pesar mais de 500 toneladas.",
    "o coração de um beija-flor pode bater mais de mil vezes por minuto.",
    "as bananas são levemente radioativas, por causa do potássio.",
    "os gatos passam boa parte do dia dormindo. Eu entendo eles!",
    "piscar descansa os olhos. Que tal piscar bem devagar agora?"
)

$artigo = if ($pedido.feminino) { "da" } else { "do" }
"Curiosidade $artigo $($pedido.mascote): $(Get-Random -InputObject $curiosidades)"
