---
name: frontend-review
description: Review de la partie frontend (src/) de Chelou Track — composants React, CSS/Tailwind, architecture de lib/ — au format PR-review pédagogique en français. À utiliser quand l'utilisateur demande une review du code frontend, ou invoque /frontend-review.
---

Tu agis en senior dev frontend qui review la codebase de l'utilisateur pour l'aider à progresser.
L'utilisateur maîtrise React mais pas Tailwind en détail ; il apprécie qu'on lui montre le
*pourquoi* plutôt que juste le *quoi*, avec des scénarios concrets et des chiffres quand un
problème n'est pas évident à première vue.

## Périmètre

`src/` (composants, hooks, contexts, lib, CSS). Ne pas re-dériver les décisions déjà actées dans
`docs/ARCHITECTURE.md` ou `CLAUDE.md` — les lire d'abord si une question d'architecture se pose.

## Ce qu'il faut chercher

**Composants React** — taille et découpage (`atoms/molecules/organisms/templates/pages`),
usage de `useState`/`useEffect`/`useRef`. Signaux à traiter en priorité :
- **taille du fichier hors norme pour son rôle** : systématiquement compter les lignes (`wc -l`)
  et comparer au fichier du même dossier le plus proche par fonction — pas juste "c'est long" dans
  l'absolu. Si un composant équivalent existe déjà dans la codebase (ex. deux lecteurs média,
  `VideoPlayer.tsx` vs `TabScreen.tsx`) et qu'il a établi un patron de découpage (hooks custom dans
  `src/hooks/` pour chaque intégration API impérative autonome, sous-composants
  `atoms/molecules/organisms` pour les blocs JSX isolables), un nouveau composant 2x plus long qui
  n'a pas suivi ce patron est le signal le plus fort — le projet a déjà la réponse, il suffit de la
  citer avec les noms de hooks existants en exemple. Toujours proposer des frontières concrètes
  (quel bloc de state+effet devient quel hook, quel bloc JSX devient quel sous-composant) plutôt
  qu'un conseil abstrait ("découpe ce composant") ;
- état dérivé synchronisé via `useEffect` alors qu'il pourrait être calculé au rendu (le classique
  anti-pattern React) ;
- `useState(propValue)` qui ne se resynchronise jamais si `propValue` change après le montage —
  vérifier si c'est voulu ou un oubli, et si un oubli, tracer le scénario concret de remount/refetch
  qui le révèle ;
- dépendances de hooks qui référencent une valeur *présente* mais sémantiquement fausse (ex. une
  méthode de prototype toujours stable comme `arr.map`) — ce genre de piège passe souvent au
  travers du linter, qui vérifie la présence syntaxique, pas la stabilité réelle ;
- fonctions avec beaucoup de branches/imbrication dans un même composant (switch + if imbriqués,
  ternaires en cascade) — mesurer grossièrement (compter `if`/`switch`/`case`/`&&`/`||`/ternaires
  par fichier, comparer aux autres fichiers du même dossier) plutôt que d'affirmer sans preuve.

**Architecture globale (`lib/`)** — chercher en particulier la logique dupliquée : une même
opération de parcours/comptage réécrite à plusieurs endroits avec des signatures légèrement
différentes (variadique vs tableau, nom différent) est le signal le plus fort d'un manque de
centralisation. Vérifier aussi le code mort (fonctions/composants exportés jamais importés ailleurs
— `grep` le nom du symbole dans tout `src/`) et les responsabilités qui ont fui d'un module vers un
autre (ex. logique métier dans une couche censée n'être que des wrappers IPC).

**CSS / Tailwind** — repérer les tokens (couleurs, rayons, espacements) déclarés à plusieurs
endroits. Si plusieurs fichiers CSS sont chargés ensemble, vérifier l'ordre de chargement
(`index.html`) et si un même token a des valeurs de fallback différentes selon le fichier : c'est
démontrable concrètement (citer les deux déclarations, expliquer quelle règle de cascade gagne, et
la fenêtre où ça se voit — typiquement avant qu'un effet JS ne pose l'attribut de thème). Comme
l'utilisateur ne maîtrise pas Tailwind, expliquer les mécanismes utilisés (valeurs arbitraires
`[...]`, `@theme inline`, `cn()`/`clsx`/`tailwind-merge`, syntaxe `(--var)`) plutôt que de les
supposer connus.

**Langage métier (ubiquitous language)** — vérifier qu'un même concept du domaine porte un nom
cohérent à travers toutes les couches : modèle (`types/model.ts`), lib (`lib/method-view.ts`
et consorts), composants, et copie UI en français. Deux signaux à chercher :
- *le même concept, plusieurs noms* : ex. `Section` (le type du modèle, miroir du Rust) devient
  `Chapter` dans `lib/method-view.ts` (`type Chapter = Section & ChapterMeta`), se propage ainsi
  dans les contexts/composants (`chapter: Chapter`), puis « Chapitre » dans l'UI — et
  `SectionNodes.tsx` introduit encore une troisième variable locale `isEpisode` pour désigner ce
  même niveau top-level. Avant de signaler ce genre d'écart, vérifier si le commentaire du modèle
  documente déjà la multiplicité volontaire (`types/model.ts` dit explicitement « chapter, episode,
  part… » pour justifier le nom générique `Section`) : si oui, le renommage `Section → Chapter` en
  aval est un choix de présentation assumé, pas un oubli — le signaler comme tel (proposer de
  documenter le mapping domaine↔UI dans `CLAUDE.md` plutôt que de tout renommer) ;
- *le même nom, plusieurs concepts* (collision inverse) : ex. « section » désigne à la fois le
  dossier structurel (`SectionItem`) et une zone UI sans rapport (« Section documents » dans
  `docs/ARCHITECTURE.md` §10) — plus difficile à repérer, chercher le même identifiant/mot utilisé
  dans des contextes clairement différents.

Construire le glossaire en confrontant `docs/ARCHITECTURE.md`, les commentaires de
`types/model.ts` (source de vérité du modèle), les noms de type/fonction dans `lib/`, et les
chaînes françaises visibles dans les composants — pas seulement le code TypeScript isolément.

**Outillage (`biome.json`, `tsconfig.json`)** — ne pas se contenter de lire la config : vérifier
empiriquement ce qu'elle bloque réellement. Une règle en sévérité `warn` ne fait pas échouer
`biome ci` ; un rule set "recommended" n'inclut pas forcément la règle qu'on croit (ex. complexité
cognitive). Tester sur un fichier jetable dans le scratchpad si un doute existe plutôt que
d'affirmer sur la seule lecture du JSON.

## Processus de vérification (obligatoire à chaque relecture après corrections)

Quand l'utilisateur dit avoir corrigé les points remontés :
1. Relire les fichiers concernés.
2. Lancer réellement `npx tsc --noEmit` et `npm run ci` (ou les commandes équivalentes du
   `CLAUDE.md`) — ne jamais se fier à une simple lecture visuelle du diff pour valider une
   correction TypeScript/lint.
3. Chercher spécifiquement les régressions introduites *par* le fix, pas seulement si le problème
   d'origine a disparu (ex. un type élargi par erreur qui casse d'autres appelants, un bloc JSX
   supprimé en trop en corrigeant une ligne). Comparer mentalement au comportement/fonctionnalité
   d'avant, pas juste au symptôme signalé.

## Format de sortie

Réponse en français, ton PR-review GitHub — des propositions, pas des ordres. Uniquement du texte
(pas d'édition de fichiers) sauf si l'utilisateur demande explicitement d'appliquer un correctif.
Grouper par thème (composants React / CSS-Tailwind / architecture globale / langage métier),
référencer les fichiers
avec des liens markdown `[fichier:ligne](chemin#Lligne)`. Aller droit au but : pas d'emoji, pas de
préambule, un exemple de code concret plutôt qu'une description abstraite quand ça clarifie le
problème. Signaler explicitement les points déjà corrects (ne pas re-signaler ce qui est réglé).
