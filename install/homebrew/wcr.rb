class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.93"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.93/wcr-macos-x86_64.tar.gz"
      sha256 "0bfa23fda4f34cf811e9d72c4a5b6db3fcc434d06fa5835d890195fc81f15888"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.93/wcr-macos-aarch64.tar.gz"
      sha256 "9e2f44f8edc53fd184262174366cb7c32155b3f83c2e0b77f18e27397ae62745"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.93/wcr-linux-x86_64.tar.gz"
      sha256 "30a74dbe48e5c7d3ff497cecdc6b2a72838be691804f06b5271449a5ea0e896f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.93/wcr-linux-aarch64.tar.gz"
      sha256 "87309a8e8948dd95202d66c66e07ef6c73aafa2b993de02ec54dc423a7ec97e2"
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
