class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.110"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.110/wcr-macos-x86_64.tar.gz"
      sha256 "aa2ed7c67182a7f06a3b2154a08f27ddc435ed9a33215a0158cb6b68b9d36c7f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.110/wcr-macos-aarch64.tar.gz"
      sha256 "3e0980f1094994d027112786fda85a01d8006ccd7d1d57417388d937ce866960"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.110/wcr-linux-x86_64.tar.gz"
      sha256 "0dd04cb58049374fdef84c9af7771ef014f14ba95df4ef837c27f6fb2083c2ec"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.110/wcr-linux-aarch64.tar.gz"
      sha256 "5a07c3b4f1230fc92bfda4c7643d970721219d3c670ea4bff897b3aaca390408"
    end
  end

  def install
    bin.install "wcr"
    bin.install "wcr-gui" if File.exist?("wcr-gui")
    bin.install "modem73" if File.exist?("modem73")
    bin.install "radio.py"
  end

  def caveats
    <<~EOS
      Run `wcr setup` once, then `wcr gui` or `wcr tui`.
      Update sha256 lines in this formula after each tagged release (see release CI).
    EOS
  end

  test do
    system "#{bin}/wcr", "--version"
  end
end
