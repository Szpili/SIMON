# Wdrożenie publicznego demo

Dwie usługi użytkownika (systemd `--user`) plus tunel. Nic tu nie wymaga roota.

```bash
# 1. ziarno węzła — TRWAŁE, bo z niego wynika zarówno node_id (podpis receiptu),
#    jak i peer_id (adres w sieci). Zgubisz je = zmienia się publiczny adres.
mkdir -p ~/.simon && chmod 700 ~/.simon
python3 -c "import secrets;print(secrets.token_hex(32))" > ~/.simon/node.key
chmod 600 ~/.simon/node.key

# 2. adres węzła dla demo (peer_id wypisuje węzeł przy starcie)
echo "SIMON_NODE=/ip4/127.0.0.1/tcp/9401/p2p/<peer_id>" > ~/.simon/env
chmod 600 ~/.simon/env

# 3. usługi
cp deploy/simon-*.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now simon-node simon-demo
loginctl enable-linger "$USER"      # bez tego usługi giną po wylogowaniu

# 4. tunel (Tailscale Funnel; konfiguracja jest trwała między restartami)
tailscale funnel --bg --https=8443 8602
```

## Czego NIE robić

`--key <hex>` w jednostce systemd. systemd rozwija `${ZMIENNA}` do argv, więc
ziarno węzła widzi wtedy każdy użytkownik maszyny przez `ps`. Zawsze `--key-file`.

Wystawiać demo bez `SIMON_PUBLIC=1`. Ten tryb blokuje edycję adresu węzła
(inaczej odwiedzający każe serwerowi łączyć się z dowolnym adresem) i włącza
globalną przepustnicę — za każdym zleceniem stoi prawdziwe GPU.

## Zależność, o której łatwo zapomnieć

Węzeł potrzebuje serwera modelu pod `--model-url`. Jeśli ten serwer nie wstaje
sam po restarcie maszyny, demo wstanie i będzie zwracać błędy — usługa będzie
„aktywna", a mimo to bezużyteczna.
