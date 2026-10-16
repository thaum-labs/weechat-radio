class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.91"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.91/wcr-macos-x86_64.tar.gz"
      sha256 "f91c9c059b285faa9b1fb4445b66de7d91b1c62a3df18a5df4bf380897d049df"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.91/wcr-macos-aarch64.tar.gz"
      sha256 "a1ffc6ba6026093f1d4da8527af0e295619241afb2ccf58433289389d2dec656"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.91/wcr-linux-x86_64.tar.gz"
      sha256 "e68d15dbc8c2194ffe0a3b15cec6e2faec177c117355da4241b903975154d2a6"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.91/wcr-linux-aarch64.tar.gz"
      sha256 "d4dd8f8d801fefcd18854b79c033a4f3b97feb445b9fa343333b7d32d1fb5f7f"
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
