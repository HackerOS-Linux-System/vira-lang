use vira_parser::ast::*;
use std::path::Path;
use anyhow::Result;

// ─── Android project output ───────────────────────────────────────────────────

pub struct AndroidOutput {
    pub kotlin_files: Vec<(String, String)>,  // (relative path, content)
    pub gradle_files: Vec<(String, String)>,
    pub manifest: String,
    pub package_name: String,
    pub app_name: String,
    pub version: String,
}

impl AndroidOutput {
    pub fn write_to(&self, out_dir: &Path) -> Result<()> {
        use anyhow::Context;
        for (rel, content) in &self.kotlin_files {
            let path = out_dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        }
        for (rel, content) in &self.gradle_files {
            let path = out_dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        }
        let manifest_path = out_dir.join("app/src/main/AndroidManifest.xml");
        std::fs::create_dir_all(manifest_path.parent().unwrap())?;
        std::fs::write(&manifest_path, &self.manifest)?;

        // gradle wrapper properties
        let wrapper_dir = out_dir.join("gradle/wrapper");
        std::fs::create_dir_all(&wrapper_dir)?;
        std::fs::write(wrapper_dir.join("gradle-wrapper.properties"), GRADLE_WRAPPER_PROPS)?;

        // gradlew script (Unix)
        std::fs::write(out_dir.join("gradlew"), GRADLEW_SCRIPT)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(out_dir.join("gradlew"))?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(out_dir.join("gradlew"), perms)?;
        }

        Ok(())
    }

    pub fn apk_build_instructions(&self, out_dir: &Path) -> String {
        format!(
            "cd {dir}\n./gradlew assembleRelease\n# APK: {dir}/app/build/outputs/apk/release/app-release.apk",
            dir = out_dir.display()
        )
    }
}

// ─── Main transpile entry point ───────────────────────────────────────────────

pub fn transpile_to_android(
    program: &Program,
    package: &str,
    app_name: &str,
    version: &str,
) -> AndroidOutput {
    let cg = KotlinCodegen::new(package, app_name, version);
    let main_kt = cg.generate_main_activity(program);
    let models_kt = cg.generate_models(program);
    let viewmodel_kt = cg.generate_viewmodel(program);

    let pkg_path = package.replace('.', "/");

    let kotlin_files = vec![
        (format!("app/src/main/kotlin/{pkg_path}/MainActivity.kt"), main_kt),
        (format!("app/src/main/kotlin/{pkg_path}/Models.kt"), models_kt),
        (format!("app/src/main/kotlin/{pkg_path}/ViewModel.kt"), viewmodel_kt),
    ];

    let gradle_files = vec![
        ("build.gradle.kts".into(),          gen_root_gradle()),
        ("app/build.gradle.kts".into(),      gen_app_gradle(package, version)),
        ("settings.gradle.kts".into(),       gen_settings_gradle(app_name)),
        ("gradle.properties".into(),         GRADLE_PROPERTIES.into()),
        ("local.properties".into(),          "sdk.dir=/opt/android-sdk\n".into()),
    ];

    AndroidOutput {
        kotlin_files,
        gradle_files,
        manifest: gen_android_manifest(package, app_name),
        package_name: package.to_owned(),
        app_name: app_name.to_owned(),
        version: version.to_owned(),
    }
}

// ─── Kotlin codegen ───────────────────────────────────────────────────────────

struct KotlinCodegen {
    package: String,
    app_name: String,
    version: String,
}

impl KotlinCodegen {
    fn new(package: &str, app_name: &str, version: &str) -> Self {
        KotlinCodegen {
            package: package.to_owned(),
            app_name: app_name.to_owned(),
            version: version.to_owned(),
        }
    }

    fn generate_main_activity(&self, program: &Program) -> String {
        let mut out = self.file_header();
        out.push_str("import android.os.Bundle\n");
        out.push_str("import androidx.activity.ComponentActivity\n");
        out.push_str("import androidx.activity.compose.setContent\n");
        out.push_str("import androidx.compose.foundation.layout.*\n");
        out.push_str("import androidx.compose.foundation.lazy.LazyColumn\n");
        out.push_str("import androidx.compose.foundation.lazy.items\n");
        out.push_str("import androidx.compose.material3.*\n");
        out.push_str("import androidx.compose.runtime.*\n");
        out.push_str("import androidx.compose.ui.Modifier\n");
        out.push_str("import androidx.compose.ui.unit.dp\n");
        out.push_str("import androidx.lifecycle.viewmodel.compose.viewModel\n\n");

        out.push_str(&format!("class MainActivity : ComponentActivity() {{\n"));
        out.push_str("    override fun onCreate(savedInstanceState: Bundle?) {\n");
        out.push_str("        super.onCreate(savedInstanceState)\n");
        out.push_str("        setContent {\n");
        out.push_str("            MaterialTheme {\n");
        out.push_str("                AppScreen()\n");
        out.push_str("            }\n");
        out.push_str("        }\n");
        out.push_str("    }\n}\n\n");

        // Generate composables from pub functions
        for item in &program.items {
            if let Item::Function(f) = item {
                if f.visibility == Visibility::Public {
                    out.push_str(&self.gen_composable(f));
                }
            }
        }

        out
    }

    fn generate_models(&self, program: &Program) -> String {
        let mut out = self.file_header();
        out.push_str("import androidx.compose.runtime.mutableStateListOf\n");
        out.push_str("import androidx.compose.runtime.mutableStateOf\n\n");

        for item in &program.items {
            match item {
                Item::Struct(s) => out.push_str(&self.gen_data_class(s)),
                Item::Enum(e)   => out.push_str(&self.gen_sealed_class(e)),
                _ => {}
            }
        }
        out
    }

    fn generate_viewmodel(&self, program: &Program) -> String {
        let mut out = self.file_header();
        out.push_str("import androidx.lifecycle.ViewModel\n");
        out.push_str("import androidx.compose.runtime.mutableStateListOf\n");
        out.push_str("import androidx.compose.runtime.getValue\n");
        out.push_str("import androidx.compose.runtime.mutableStateOf\n");
        out.push_str("import androidx.compose.runtime.setValue\n\n");

        out.push_str("class AppViewModel : ViewModel() {\n");

        // Generate state and functions from impl blocks
        for item in &program.items {
            match item {
                Item::Impl(imp) => {
                    for method in &imp.items {
                        if let Item::Function(f) = method {
                            if f.visibility == Visibility::Public {
                                out.push_str(&format!("    {}\n", self.gen_vm_fn(f)));
                            }
                        }
                    }
                }
                Item::Function(f) if f.visibility == Visibility::Public => {
                    out.push_str(&format!("    {}\n", self.gen_vm_fn(f)));
                }
                _ => {}
            }
        }

        out.push_str("}\n");
        out
    }

    fn file_header(&self) -> String {
        format!(
            "// Generated by Vira v0.1.0 — DO NOT EDIT\npackage {}\n\n",
            self.package
        )
    }

    fn gen_data_class(&self, s: &StructDef) -> String {
        let fields: Vec<String> = s.fields.iter().map(|f| {
            format!("    val {}: {}", f.name, kt_type(&f.ty))
        }).collect();
        if fields.is_empty() {
            format!("data class {}()\n\n", s.name)
        } else {
            format!("data class {}(\n{}\n)\n\n", s.name, fields.join(",\n"))
        }
    }

    fn gen_sealed_class(&self, e: &EnumDef) -> String {
        let mut out = format!("sealed class {} {{\n", e.name);
        for v in &e.variants {
            match &v.fields {
                EnumVariantFields::Unit => {
                    out.push_str(&format!("    object {} : {}()\n", v.name, e.name));
                }
                EnumVariantFields::Tuple(types) => {
                    let ps: Vec<String> = types.iter().enumerate()
                    .map(|(i, t)| format!("val f{i}: {}", kt_type(t)))
                    .collect();
                    out.push_str(&format!("    data class {}({}) : {}()\n",
                                          v.name, ps.join(", "), e.name));
                }
                EnumVariantFields::Struct(fields) => {
                    let ps: Vec<String> = fields.iter()
                    .map(|f| format!("val {}: {}", f.name, kt_type(&f.ty)))
                    .collect();
                    out.push_str(&format!("    data class {}({}) : {}()\n",
                                          v.name, ps.join(", "), e.name));
                }
            }
        }
        out.push_str("}\n\n");
        out
    }

    fn gen_composable(&self, f: &FunctionDef) -> String {
        let params: Vec<String> = f.params.iter().filter(|p| !p.is_self)
        .map(|p| format!("{}: {}", p.name, kt_type(&p.ty)))
        .collect();
        let mut out = format!("@Composable\nfun {}({}) {{\n", f.name, params.join(", "));
        out.push_str("    // Generated from Vira UI function\n");
        out.push_str("    val vm: AppViewModel = viewModel()\n");
        if let Some(body) = &f.body {
            for stmt in &body.stmts {
                out.push_str(&format!("    {}\n", gen_kt_stmt(stmt)));
            }
        }
        out.push_str("}\n\n");
        out
    }

    fn gen_vm_fn(&self, f: &FunctionDef) -> String {
        let params: Vec<String> = f.params.iter().filter(|p| !p.is_self)
        .map(|p| format!("{}: {}", p.name, kt_type(&p.ty)))
        .collect();
        let ret = f.return_type.as_ref()
        .map(|t| format!(": {}", kt_type(t)))
        .unwrap_or_default();
        let mut out = format!("fun {}({}){} {{\n", f.name, params.join(", "), ret);
        if let Some(body) = &f.body {
            for stmt in &body.stmts {
                out.push_str(&format!("        {}\n", gen_kt_stmt(stmt)));
            }
        }
        out.push_str("    }");
        out
    }
}

// ─── Kotlin type mapping ──────────────────────────────────────────────────────

pub fn kt_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(n, args) => {
            let kn = match n.as_str() {
                "i8" | "i16" | "i32" => "Int",
                "i64" | "i128"       => "Long",
                "u8" | "u16" | "u32" => "UInt",
                "u64" | "u128"       => "ULong",
                "usize" | "isize"    => "Long",
                "f32"                => "Float",
                "f64"                => "Double",
                "bool"               => "Boolean",
                "String" | "str"     => "String",
                "char"               => "Char",
                other                => other,
            };
            if args.is_empty() { kn.to_owned() }
            else { format!("{}<{}>", kn, args.iter().map(kt_type).collect::<Vec<_>>().join(", ")) }
        }
        TypeExpr::Optional(inner)  => format!("{}?", kt_type(inner)),
        TypeExpr::Result(ok, _)    => format!("Result<{}>", kt_type(ok)),
        TypeExpr::Slice(inner)     => format!("MutableList<{}>", kt_type(inner)),
        TypeExpr::Tuple(ts) if ts.len() == 2 => {
            format!("Pair<{}>", ts.iter().map(kt_type).collect::<Vec<_>>().join(", "))
        }
        TypeExpr::Void | TypeExpr::Never => "Unit".into(),
        TypeExpr::SelfTy  => "Self".into(),
        TypeExpr::Infer   => "Any".into(),
        TypeExpr::Ref(inner) | TypeExpr::RefMut(inner) => kt_type(inner),
        _ => "Any".into(),
    }
}

fn gen_kt_stmt(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Let(l) => {
            let name = match &l.name { Pattern::Ident(n) => n.clone(), _ => "_".into() };
            let val = l.value.as_ref().map(|v| format!(" = {}", gen_kt_expr(v))).unwrap_or_default();
            format!("val {name}{val}")
        }
        Stmt::Var(v) => {
            let name = match &v.name { Pattern::Ident(n) => n.clone(), _ => "_".into() };
            let val = v.value.as_ref().map(|e| format!(" = {}", gen_kt_expr(e))).unwrap_or_default();
            format!("var {name}{val}")
        }
        Stmt::Return(Some(e), _) => format!("return {}", gen_kt_expr(e)),
        Stmt::Return(None, _)    => "return".into(),
        Stmt::Expr(e)            => gen_kt_expr(e),
        Stmt::Throw(e, _)        => format!("throw Exception(\"{}\")", gen_kt_expr(e)),
        _ => "// stmt".into(),
    }
}

fn gen_kt_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match lit {
            LiteralKind::Int(n)   => n.to_string(),
            LiteralKind::Float(f) => format!("{f}"),
            LiteralKind::Str(s)   => format!("\"{}\"", s.replace('"', "\\\"")),
            LiteralKind::Char(c)  => format!("'{c}'"),
            LiteralKind::Bool(b)  => b.to_string(),
            LiteralKind::Nil      => "null".into(),
        },
        ExprKind::Ident(n)   => n.clone(),
        ExprKind::Path(segs) => segs.join("."),
        ExprKind::Binary(op, l, r) => {
            let ls = gen_kt_expr(l);
            let rs = gen_kt_expr(r);
            let ops = match op {
                BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
                BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq  => "==",
                BinOp::NotEq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                BinOp::LtEq => "<=", BinOp::GtEq => ">=",
                BinOp::And => "&&", BinOp::Or => "||",
                _ => "+"
            };
            format!("({ls} {ops} {rs})")
        }
        ExprKind::Call(callee, args) => {
            let c = gen_kt_expr(callee);
            let a: Vec<String> = args.iter().map(|a| gen_kt_expr(&a.value)).collect();
            format!("{c}({})", a.join(", "))
        }
        ExprKind::MethodCall(recv, method, _, args) => {
            let r = gen_kt_expr(recv);
            let a: Vec<String> = args.iter().map(|a| gen_kt_expr(&a.value)).collect();
            format!("{r}.{method}({})", a.join(", "))
        }
        ExprKind::Field(obj, field) => format!("{}.{field}", gen_kt_expr(obj)),
        ExprKind::If(cond, then, elifs, else_) => {
            let mut s = format!("if ({}) {{\n", gen_kt_expr(cond));
            s.push_str("    /* then */\n}");
            if else_.is_some() { s.push_str(" else {\n    /* else */\n}"); }
            s
        }
        ExprKind::StructLit(name, fields) => {
            let fs: Vec<String> = fields.iter()
            .map(|(k,v)| format!("{k} = {}", gen_kt_expr(v)))
            .collect();
            format!("{name}({})", fs.join(", "))
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(gen_kt_expr).collect();
            format!("mutableListOf({})", es.join(", "))
        }
        ExprKind::Closure(params, _, body) => {
            let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            format!("{{ {} -> {} }}", ps.join(", "), gen_kt_expr(body))
        }
        ExprKind::Await(e) => format!("{} /* await */", gen_kt_expr(e)),
        ExprKind::Try(e)   => format!("{}!!", gen_kt_expr(e)),
        ExprKind::Unary(op, e) => match op {
            UnaryOp::Not => format!("!{}", gen_kt_expr(e)),
            UnaryOp::Neg => format!("-{}", gen_kt_expr(e)),
            _ => gen_kt_expr(e),
        },
        ExprKind::Block(b) => {
            let stmts: Vec<String> = b.stmts.iter().map(gen_kt_stmt).collect();
            let tail = b.tail.as_ref().map(|e| gen_kt_expr(e)).unwrap_or_default();
            format!("run {{\n{}\n{tail}\n}}", stmts.join("\n"))
        }
        _ => "/* expr */".into(),
    }
}

// ─── Gradle file generation ───────────────────────────────────────────────────

fn gen_root_gradle() -> String {
    r#"plugins {
    id("com.android.application") version "8.2.2" apply false
    id("org.jetbrains.kotlin.android") version "1.9.22" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "1.9.22" apply false
}
"#.into()
}

fn gen_app_gradle(package: &str, version: &str) -> String {
    format!(r#"plugins {{
        id("com.android.application")
        id("org.jetbrains.kotlin.android")
        id("org.jetbrains.kotlin.plugin.compose")
}}

android {{
namespace = "{package}"
compileSdk = 34

defaultConfig {{
applicationId = "{package}"
minSdk = 24
targetSdk = 34
versionCode = 1
versionName = "{version}"
testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
}}

buildTypes {{
release {{
isMinifyEnabled = true
proguardFiles(
    getDefaultProguardFile("proguard-android-optimize.txt"),
            "proguard-rules.pro"
    )
}}
}}

compileOptions {{
sourceCompatibility = JavaVersion.VERSION_1_8
targetCompatibility = JavaVersion.VERSION_1_8
}}

kotlinOptions {{
jvmTarget = "1.8"
}}

buildFeatures {{
compose = true
}}
}}

dependencies {{
implementation("androidx.core:core-ktx:1.12.0")
        implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.7.0")
        implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.7.0")
        implementation("androidx.activity:activity-compose:1.8.2")
        implementation(platform("androidx.compose:compose-bom:2024.01.00"))
        implementation("androidx.compose.ui:ui")
        implementation("androidx.compose.ui:ui-graphics")
        implementation("androidx.compose.ui:ui-tooling-preview")
        implementation("androidx.compose.material3:material3")
        implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")
        debugImplementation("androidx.compose.ui:ui-tooling")
        testImplementation("junit:junit:4.13.2")
        androidTestImplementation("androidx.test.ext:junit:1.1.5")
}}
"#)
}

fn gen_settings_gradle(app_name: &str) -> String {
    let clean = app_name.replace(' ', "-").to_lowercase();
    format!(r#"pluginManagement {{
        repositories {{
        google()
        mavenCentral()
        gradlePluginPortal()
}}
}}
dependencyResolutionManagement {{
repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
        repositories {{
        google()
        mavenCentral()
}}
}}
rootProject.name = "{clean}"
include(":app")
        "#)
}

fn gen_android_manifest(_package: &str, app_name: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="utf-8"?>
        <manifest xmlns:android="http://schemas.android.com/apk/res/android">
        <application
        android:allowBackup="true"
        android:icon="@mipmap/ic_launcher"
        android:label="{app_name}"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:supportsRtl="true"
        android:theme="@style/Theme.Material3.DynamicColors.DayNight">
        <activity
        android:name=".MainActivity"
        android:exported="true"
        android:theme="@style/Theme.Material3.DynamicColors.DayNight">
        <intent-filter>
        <action android:name="android.intent.action.MAIN" />
        <category android:name="android.intent.category.LAUNCHER" />
        </intent-filter>
        </activity>
        </application>
        </manifest>
        "#)
}

const GRADLE_WRAPPER_PROPS: &str = r#"distributionBase=GRADLE_USER_HOME
distributionPath=wrapper/dists
distributionUrl=https\://services.gradle.org/distributions/gradle-8.4-bin.zip
networkTimeout=10000
validateDistributionUrl=true
zipStoreBase=GRADLE_USER_HOME
zipStorePath=wrapper/dists
"#;

const GRADLE_PROPERTIES: &str = r#"org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
android.nonTransitiveRClass=true
"#;

const GRADLEW_SCRIPT: &str = r#"#!/bin/sh
# Gradle wrapper startup script
exec java -jar "$(dirname "$0")/gradle/wrapper/gradle-wrapper.jar" "$@"
"#;
