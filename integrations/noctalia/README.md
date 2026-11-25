# Sincronização com Noctalia 5.4+

O Noctalia renderiza os tokens Material 3 a cada mudança de esquema, modo ou wallpaper. O Acervo observa alterações no diretório do arquivo gerado e anima a paleta sem reiniciar. Uma consulta de contingência a cada dois segundos cobre arquivos ou diretórios criados posteriormente e eventos perdidos. A integração é somente leitura; o Acervo não modifica o wallpaper nem a configuração do Noctalia.

## Instalação (Linux + Noctalia)

```sh
mkdir -p "$HOME/.config/noctalia/templates" "$HOME/.config/acervo"
cp integrations/noctalia/acervo.json "$HOME/.config/noctalia/templates/acervo.json"
cp integrations/noctalia/acervo.toml "$HOME/.config/noctalia/acervo.toml"
```

Os dois arquivos são **aditivos**: não substituem `config.toml` nem as preferências gerenciadas pelo Noctalia.

O Noctalia v5 carrega automaticamente arquivos `*.toml` da pasta de configuração. Depois de gerar a paleta, procure o arquivo `~/.config/acervo/noctalia-colors.json`.

Em **Configurações > Aparência**, selecione a paleta **Automática · Noctalia**. É possível combiná-la tanto com Liquid Glass quanto com Material 3.

Se desejar remover a integração, retire somente o arquivo `~/.config/noctalia/acervo.toml` e o template `~/.config/noctalia/templates/acervo.json`. O Acervo continua funcionando com as paletas manuais.
