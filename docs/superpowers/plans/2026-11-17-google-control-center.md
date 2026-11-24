# AI Usage Hub Google kontrol merkezi

Kullanıcının 17 Kasım 2026 metni AI Usage Hub'ın mevcut Google/Antigravity özelliklerine eklenir. Uygulama mevcut Tauri, React, TypeScript, Rust ve SQLite yapısını kullanır. Mevcut Antigravity girişleri, IDE geçişi ve kota okuması korunur. Yeni veriler aynı veritabanındaki google_* tablolarında tutulur.

## Uygulama sırası

1. Google hesapları için metadata, abonelik, etiket, grup, favori, arşivleme ve etkinlik tablolarını ekle. Mevcut Antigravity hesaplarını e-posta ile tekilleştirerek bağla. Arşivlenmiş kaydı eşitlemede tekrar etkinleştirme.
2. Hesap başına kalıcı Chrome/Edge kullanıcı dizini oluştur. Yalnız sekiz tanımlı resmî hizmet URL'sini aç. Yerel tarayıcı kendi çerezlerini tutar. Başlatma kayıtlarını ve hizmetin son hesabını yalnız başarılı işlemden sonra güncelle. Oturum/hizmet durumu kullanıcı onayı olarak açık etiketlenir.
3. Proje ve hesap/rol/hizmet atamalarını gerçek SQLite işlemleriyle ekle. Kullanım sayıları yalnız native Antigravity kaynağından veya açıkça manuel girişten gelir. Manuel kaydın ölçüm zamanı ve geçmişi korunur.
4. JSON/CSV metadata aktarımını ekle. İçe aktarımları tümü doğrulandıktan sonra tek işlemde uygula. Token, parola ve tarayıcı oturumları metadata çıktısına dahil edilmez.
5. SQLite'ın backup API'siyle tam veritabanı yedekleme, doğrulanmış geri yükleme, geri yükleme öncesi yedek ve günlük otomatik yedekleri ekle. Yerel yedek konumu uygulama veri dizinidir. Otomatik saklama sayısı ve son kullanma uyarıları ayarlanabilir.
6. Google kontrol merkezi sayfasında Genel Bakış, Hesaplar, Hizmetler, Projeler, Kullanım, Etkinlik ve Ayarlar sekmelerini ekle. Mevcut bordo ve yıldızlı tema kullanılır. Grid/tablo, arama/filtre, abonelik uyarısı ve klavye kısayolları gerçek işlemlere bağlanır.
7. Ana gezinmeye ve mevcut Antigravity hesap bölümüne giriş ekle. Mevcut tema ayarını uygula.
8. Hesap/proje CRUD, metadata aktarımı, veri bütünlüğü, tarayıcı profil ayrılığı ve yedek/geri yükleme için testler çalıştır. Başsız tarayıcıda normal kullanıcı akışlarını ve dar pencere düzenini doğrula. Windows paketini derle ve sessiz kurulumla doğrula.

## İş bölümü ve dosyalar

- src/lib/googleControl.ts ortak IPC ve TypeScript sözleşmesidir.
- src-tauri/src/google_control/ metadata, proje, kullanım, aktarım ve komut modellerinin sahibi olur.
- src/pages/google/ ve src/pages/GoogleControlPage.tsx yeni arayüzü içerir.
- Tarayıcı ve yedekleme src-tauri/src/google_browser.rs ve google_backups.rs dosyalarında kalır.
- Ana ajan veritabanı bağlantısı, komut kaydı, gezinme, genel tema, son entegrasyon, doğrulama ve paketlemeyi yapar.

Gerçek Google girişleri kullanıcı tarafından ayrı tarayıcıda tamamlanır. API erişimi bulunmayan Flow, Gemini, NotebookLM ve Drive bilgileri manuel kayıt şeklinde sunulur.
