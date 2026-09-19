class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-x86_64.tar.gz"
      sha256 "48856fa42ea501a7281fa70f73ce8ffab2b95247cf05688d58dd522a858d1397"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-aarch64.tar.gz"
      sha256 "59da9afe0d7d0ce05457ebf3df11b29740e3821650fb5ada48fd67f9e2ef4975"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-x86_64.tar.gz"
      sha256 "da527161227f4fb2e0205415c5ee729091884166dfc4c7b452d25bd72e7cfde9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-aarch64.tar.gz"
      sha256 "f2c4a7cc3240a5bfff25a546187d684b29217f8b93ff63a4b50847185d7ff4c3"
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
