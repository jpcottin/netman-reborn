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

**Ce répertoire ne contient pas tout.** Un nouveau port doit aussi être
déclaré dans le `Makefile` de sa catégorie, sans quoi l'arbre ne le voit
pas. Ajouter dans `/usr/ports/net-mgmt/Makefile`, en respectant le tri
alphabétique — entre `netmagis-www` et `netmask` :

```
    SUBDIR += netman
```

Poudriere ne le signale pas, car il adresse un port par son origine.
L'oubli ne se voit qu'au `make index` ou à la soumission.

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

## Mettre le port à jour après une nouvelle version amont

Dans l'ordre, le tag amont devant exister au préalable :

```sh
# 1. DISTVERSION dans le Makefile, puis :
make makesum                      # distinfo : tarball + crates
make cargo-crates > Makefile.crates   # seulement si Cargo.lock a change
make makesum                      # a nouveau si la liste de crates a bouge
make clean && make patch          # verifier que patch-netman.1 s'applique encore
```

`make cargo-crates` écrit ses messages d'extraction sur la sortie standard :
si la liste est régénérée, ne garder que la partie à partir de
`CARGO_CRATES=`.

Ne jamais déplacer un tag amont déjà publié : `distinfo` en fige l'empreinte
SHA256, et un tag réécrit casse le port pour tout le monde.

## Validation

Version **1.0.1**, avec poudriere :

| Cible | Résultat | Cible Rust effective |
|---|---|---|
| 15.1-RELEASE amd64, `DOCS=on` | OK | `x86_64-unknown-freebsd` |
| 15.1-RELEASE amd64, `DOCS=off` | OK | — |
| 14.4-RELEASE amd64 | OK | `x86_64-unknown-freebsd` |
| 14.4-RELEASE i386 | OK | `i686-unknown-freebsd` |

`check-plist` sans écart, aucune violation de staging, aucun fichier
résiduel après désinstallation. `portlint -A -C` ne remonte rien.

```sh
poudriere testport -j 151amd64 -p local -o net-mgmt/netman
poudriere testport -j 151amd64 -p local -z nodocs -o net-mgmt/netman
portlint -A -C
```

### aarch64 : non validable sous émulation

`lang/rust` se déclare `IGNORE` sous qemu-user-static (« fails to build with
qemu-user-static »), donc poudriere le retire de la file et la dépendance ne
peut pas être satisfaite. Fournir le paquet rust aarch64 officiel débloque
`build-depends`, mais **`rustc` lui-même meurt ensuite** :

```
qemu:handle_cpu_signal received signal outside vCPU context
Abort trap (core dumped)
```

Valider un port Rust sur aarch64 exige donc du matériel ou une VM aarch64
réels. Inutile d'y passer du temps en émulation.

### Machines à grand nombre de cœurs

`MAKE_JOBS_NUMBER` n'est pas une variable poudriere : la poser dans
`poudriere.conf` n'a aucun effet. Elle doit aller dans un `make.conf`
(`/usr/local/etc/poudriere.d/make.conf`). Sans cela les ports héritent de
`hw.ncpu`, ce qui suffit à faire tuer `lang/rust` par l'OOM killer sur une
machine à 64 cœurs et 32 Go. Prévoir aussi ~35 Go de `wrkdir` transitoire
pour une compilation de rust depuis les sources.

## Soumission à FreeBSD

Par Bugzilla, pas par pull request : https://bugs.freebsd.org/bugzilla/

Produit **Ports & Packages**, composant **Individual Port(s)**, résumé
préfixé `[NEW PORT]`. Joindre un patch `git format-patch` en `text/plain`
avec la case « patch » cochée, assignee laissé vide.
