{ pkgs }: {
  deps = [
    pkgs.ruby_3_2
    pkgs.bundler
    pkgs.rustc
    pkgs.cargo
    pkgs.redis
    pkgs.sqlite
    pkgs.nodejs_20
    pkgs.pkg-config
    pkgs.openssl
    pkgs.libyaml
  ];
}
