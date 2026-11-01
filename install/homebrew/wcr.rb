class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.13"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.13/wcr-macos-x86_64.tar.gz"
      sha256 "19c859899f7e0ccb36bf2085e936a8f35122365e4a3cee583b21ccb9bfebf47c"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.13/wcr-macos-aarch64.tar.gz"
      sha256 "b648a9498044f74d9b7ba02509779d290928cee3da280ba0841f284a485d6aa6"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.13/wcr-linux-x86_64.tar.gz"
      sha256 "449c2a7b202d8dc9a2de9a4bc80fbe466f917390fe45ad35e19d1befbecc38d5"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.13/wcr-linux-aarch64.tar.gz"
      sha256 "5638d8bf5f1e8e173a73a265502905f85d91937d63fdcce704d81fcd9dea6308"
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
