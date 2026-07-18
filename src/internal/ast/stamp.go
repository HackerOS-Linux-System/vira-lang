package ast

import "reflect"

var posType = reflect.TypeOf(Pos{})

// StampFile ustawia Pos.File na `file` dla KAŻDEGO węzła w drzewie `node`,
// który niesie pole Pos - ale tylko tam, gdzie File jest jeszcze puste
// (nie nadpisuje). Bundler woła to zaraz po sparsowaniu każdego modułu,
// zanim scali go z innymi - dzięki temu diagnostyki z checkera wiedzą,
// z którego pliku pochodzą, mimo że bundler spłaszcza wiele plików w
// jeden ast.Program.
//
// Implementacja przez refleksję zamiast ręcznego wypisywania każdego typu
// AST: drzewo ma kilkanaście typów węzłów i rośnie - ręczne odwiedzanie
// każdego pola w każdym typie byłoby żmudne i kruche (łatwo zapomnieć o
// nowym polu przy kolejnej zmianie). Refleksja kosztuje trochę wydajności,
// ale StampFile woła się raz na plik, nie w gorącej pętli.
func StampFile(node interface{}, file string) {
	stampValue(reflect.ValueOf(node), file)
}

func stampValue(v reflect.Value, file string) {
	if !v.IsValid() {
		return
	}
	switch v.Kind() {
	case reflect.Ptr, reflect.Interface:
		if v.IsNil() {
			return
		}
		stampValue(v.Elem(), file)
	case reflect.Struct:
		if v.Type() == posType {
			if v.CanSet() {
				f := v.FieldByName("File")
				if f.IsValid() && f.CanSet() && f.String() == "" {
					f.SetString(file)
				}
			}
			return
		}
		for i := 0; i < v.NumField(); i++ {
			fv := v.Field(i)
			if fv.CanSet() {
				stampValue(fv, file)
			}
		}
	case reflect.Slice, reflect.Array:
		for i := 0; i < v.Len(); i++ {
			stampValue(v.Index(i), file)
		}
	case reflect.Map:
		for _, k := range v.MapKeys() {
			stampValue(v.MapIndex(k), file)
		}
	}
}
