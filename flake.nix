{
  description = "SMLwatcher3000";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = {
    self,
    nixpkgs,
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {inherit system;};
    lib = pkgs.lib;
  in {
    packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
      pname = "smlwatcher3000";
      version = "0.1.0";
      src = ./.;

      cargoLock.lockFile = ./Cargo.lock;
      cargoHash = lib.fakeHash;

      nativeBuildInputs = [
        pkgs.makeWrapper
        pkgs.pkg-config
      ];

      buildInputs = [
        pkgs.dbus
      ];

      postFixup = ''
        wrapProgram "$out/bin/smlwatcher3000" \
          --prefix PATH : "${lib.makeBinPath [ pkgs.firefox pkgs.geckodriver ]}"
      '';

      postInstall = ''
        mkdir -p "$out/share/applications"
        cat > "$out/share/applications/smlwatcher3000.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=SMLwatcher3000
Comment=Watch YouTube videos one after another
Exec=$out/bin/smlwatcher3000
Terminal=true
Categories=AudioVideo;Player;
Icon=smlwatcher3000
EOF

  mkdir -p "$out/share/pixmaps"
  install -Dm644 "$src/smlwatcher3000.png" "$out/share/pixmaps/smlwatcher3000.png"
      '';
    };

    apps.${system}.default = {
      type = "app";
      program = "${self.packages.${system}.default}/bin/smlwatcher3000";
    };

    devShells.${system}.default = pkgs.mkShell {
      packages = with pkgs; [
        cargo
        dbus
        firefox
        geckodriver
        rustc
        pkg-config
        python314Packages.yt-dlp
      ];
    };
  };
}
