// build.rb - skrypt pre-buildowy pakietu, uruchamiany przez `vira build`
// PRZED kompilacją src/. Odpowiednik build.rs z ekosystemu Cargo - stąd
// nazwa pliku (żart/konwencja: ".rb" tak jak Rust ma ".rs", mimo że treścią
// jest zwykły kod Hyper Lang, nie Ruby).
//
// Typowe zastosowania: generowanie stałych z metadanych builda (hash commita,
// data builda), sprawdzanie zależności systemowych, generowanie kodu z plików
// schematów (np. .proto, .graphql) przed właściwą kompilacją.

import { execSync } from "child_process"
import { writeFileSync } from "fs"

fn gitCommitHash() -> string {
    return execSync("git rev-parse --short HEAD").toString().trim()
}

fn buildContext(ctx: any) {
    let commit = gitCommitHash()
    let now = new Date()

    let generated = `// PLIK WYGENEROWANY PRZEZ build.rb - NIE EDYTUJ RĘCZNIE
export const BUILD_COMMIT = "${commit}";
export const BUILD_TIME = "${now.toISOString()}";
`

    writeFileSync("src/build_info.gen.js", generated)
    console.log(`build.rb: zapisano src/build_info.gen.js (commit ${commit})`)
}

buildContext(null)
