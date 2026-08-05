# Port FreeBSD — net-mgmt/netman

Sources du port FreeBSD de netman-reborn, conservées ici pour qu'elles
suivent le code qu'elles empaquettent. Le port lui-même a vocation à être
soumis à l'arbre des ports FreeBSD, dans la catégorie **net-mgmt**.

## Contenu

| Fichier | Rôle |
|---|---|
| `Makefile` | définition du port : `USES=cargo`, `USE_GITHUB`, option `DOCS` |
| `Makefile.crates` | les 147 crates figées (convention `.sinclude` du framework cargo) |
| `distinfo` | empreintes du tarball GitHub et des crates |
| `pkg-descr` | description longue du paquet |
| `files/netman.in` | script `rc.d` |
| `files/pkg-message.in` | message affiché à l'installation |
| `files/patch-netman.1` | ajouts FreeBSD à la page de manuel |

## Installation dans un arbre de ports

```sh
cp -R freebsd-port /usr/ports/net-mgmt/netman
cd /usr/ports/net-mgmt/netman
make install clean
```

## Ce que le patch de la page de manuel ajoute

`netman.1` est maintenue en amont, dans la racine du dépôt. Le patch n'y
ajoute que ce qui n'a de sens que sur FreeBSD :

- le comportement des navigateurs vis-à-vis de **WebGL**, dont sigma.js a
  besoin : `www/firefox` fonctionne tel quel, `www/linux-chrome` exige
  `--enable-unsafe-swiftshader` faute d'accès au GPU sous `linux(4)` ;
- la recette `devfs.rules(5)` pour ouvrir `bpf(4)` à un groupe ;
- les variables `rc.conf` du service et les chemins installés.

Les marqueurs `%%PREFIX%%` et `%%DATADIR%%` du patch sont substitués par la
cible `post-patch` du `Makefile`, pour que la page suive `PREFIX`.

## Validation

Le port a été validé avec poudriere sur **15.1-RELEASE amd64**, avec et sans
l'option `DOCS` : `check-plist` sans écart, aucune violation de staging,
aucun fichier résiduel après désinstallation. `portlint -A -C` ne remonte
rien.

```sh
poudriere testport -j 151amd64 -p local -o net-mgmt/netman
poudriere testport -j 151amd64 -p local -z nodocs -o net-mgmt/netman
portlint -A -C
```

À noter pour qui reconstruit sur une machine à grand nombre de cœurs :
`MAKE_JOBS_NUMBER` n'est pas une variable poudriere et doit être posée dans
un `make.conf` (`/usr/local/etc/poudriere.d/make.conf`). Sans cela les ports
héritent de `hw.ncpu`, ce qui suffit à faire tuer `lang/rust` par l'OOM
killer sur une machine à 64 cœurs et 32 Go.
