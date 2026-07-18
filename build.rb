#!/usr/bin/env ruby
# frozen_string_literal: true

# build.rb - skrypt pomocniczy przy wydaniach (release helper), napisany
# w PRAWDZIWYM Ruby (nie w Hyper Lang - odróżnia to od build.hyper, który
# jest hookiem pre-build w samym Hyper Lang, analogicznym do Cargo build.rs).
#
# Robi rzeczy, do których Ruby świetnie się nadaje jako skryptowy klej wokół
# release'u: podbija wersję w vira.yml, dopisuje wpis do CHANGELOG.md,
# tworzy tag gita. Celowo NIE dotyka samego builda (to robi `vira build`) -
# to jest narzędzie "dookoła" release'u, nie część kompilacji.
#
# Użycie:
#   ruby build.rb bump patch    # 0.1.0 -> 0.1.1
#   ruby build.rb bump minor    # 0.1.0 -> 0.2.0
#   ruby build.rb bump major    # 0.1.0 -> 1.0.0
#   ruby build.rb changelog "Opis zmiany"
#   ruby build.rb tag           # tworzy i wypisuje komendę do wypchnięcia tagu git

require 'yaml'
require 'time'
require 'fileutils'

MANIFEST_PATH = File.join(__dir__, 'vira.yml')
CHANGELOG_PATH = File.join(__dir__, 'CHANGELOG.md')

def load_manifest
  YAML.load_file(MANIFEST_PATH)
end

def save_manifest(data)
  # Ruby's Psych domyślnie sortuje/formatuje trochę inaczej niż chcielibyśmy
  # dla czytelności pliku utrzymywanego ręcznie - piszemy prosty, przewidywalny
  # format zamiast polegać na YAML.dump dla całego dokumentu.
  pkg = data['package']
  yaml = +"package:\n"
  yaml << "  name: #{pkg['name']}\n"
  yaml << "  version: #{pkg['version']}\n"
  pkg.each do |k, v|
    next if %w[name version].include?(k)
    yaml << "  #{k}: #{v}\n"
  end
  data.each do |k, v|
    next if k == 'package'
    yaml << "\n#{YAML.dump({ k => v }).sub(/\A---\n/, '')}"
  end
  File.write(MANIFEST_PATH, yaml)
end

def parse_semver(version)
  major, minor, patch = version.to_s.split('.').map(&:to_i)
  [major || 0, minor || 0, patch || 0]
end

def bump(kind)
  data = load_manifest
  major, minor, patch = parse_semver(data['package']['version'])

  case kind
  when 'major' then major += 1; minor = 0; patch = 0
  when 'minor' then minor += 1; patch = 0
  when 'patch' then patch += 1
  else
    warn "build.rb: nieznany rodzaj bumpa '#{kind}' (użyj: major|minor|patch)"
    exit 1
  end

  new_version = "#{major}.#{minor}.#{patch}"
  data['package']['version'] = new_version
  save_manifest(data)
  puts "build.rb: wersja podbita do #{new_version}"
  new_version
end

def changelog(message)
  date = Time.now.strftime('%Y-%m-%d')
  version = load_manifest['package']['version']
  entry = "## #{version} - #{date}\n\n- #{message}\n\n"

  existing = File.exist?(CHANGELOG_PATH) ? File.read(CHANGELOG_PATH) : "# Changelog\n\n"
  File.write(CHANGELOG_PATH, existing.sub("# Changelog\n\n", "# Changelog\n\n#{entry}"))
  puts "build.rb: dopisano wpis do CHANGELOG.md dla wersji #{version}"
end

def tag_command
  version = load_manifest['package']['version']
  tag = "v#{version}"
  puts "build.rb: aby otagować i wypchnąć release, uruchom:"
  puts "  git tag -a #{tag} -m \"Release #{tag}\""
  puts "  git push origin #{tag}"
end

def usage
  puts <<~USAGE
    build.rb - pomocnik release'owy (Ruby)

    Użycie:
      ruby build.rb bump major|minor|patch
      ruby build.rb changelog "opis zmiany"
      ruby build.rb tag
  USAGE
end

case ARGV[0]
when 'bump'
  bump(ARGV[1] || 'patch')
when 'changelog'
  if ARGV[1].nil?
    warn 'build.rb: użycie: ruby build.rb changelog "opis"'
    exit 1
  end
  changelog(ARGV[1])
when 'tag'
  tag_command
else
  usage
end
